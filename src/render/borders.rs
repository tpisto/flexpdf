//! Border and background rendering helpers.

use crate::pdf::ContentStream;
use crate::layout::LayoutRect;
use crate::style::{BorderStyle, Color, Style};

pub(super) fn render_view_borders(
    content: &mut ContentStream,
    layout: &LayoutRect,
    style: &Style,
    page_height: f32,
) {
    // Convert to PDF coordinates (flip Y)
    let pdf_x = layout.x;
    let pdf_y = page_height - layout.y - layout.height;
    let radius_tl = style.border_top_left_radius();
    let radius_tr = style.border_top_right_radius();
    let radius_br = style.border_bottom_right_radius();
    let radius_bl = style.border_bottom_left_radius();
    let border_style = style.border_style.unwrap_or(BorderStyle::Solid);

    let apply_border_style = |content: &mut ContentStream, style: BorderStyle, width: f32| {
        match style {
            BorderStyle::Solid => {
                content.set_line_dash(&[], 0.0);
                content.set_line_cap(0);
            }
            BorderStyle::Dashed => {
                let dash = (width * 2.0).max(1.0);
                let gap = (width * 1.2).max(1.0);
                content.set_line_dash(&[dash, gap], 0.0);
                content.set_line_cap(0);
            }
            BorderStyle::Dotted => {
                let dot = width.max(1.0);
                let gap = (width * 1.2).max(1.0);
                content.set_line_dash(&[dot, gap], 0.0);
                content.set_line_cap(0);
            }
        }
    };

    // Draw background fill if specified
    if let Some(bg_color) = &style.background_color {
        content.save();
        content.set_fill_color(bg_color.r, bg_color.g, bg_color.b);
        if radius_tl > 0.0 || radius_tr > 0.0 || radius_br > 0.0 || radius_bl > 0.0 {
            content.rounded_rect_corners(
                pdf_x,
                pdf_y,
                layout.width,
                layout.height,
                radius_tl,
                radius_tr,
                radius_br,
                radius_bl,
            );
        } else {
            content.rect(pdf_x, pdf_y, layout.width, layout.height);
        }
        content.fill();
        content.restore();
    }

    // Draw borders (support per-side widths/colors)
    let bw_top = style.border_top_width();
    let bw_right = style.border_right_width();
    let bw_bottom = style.border_bottom_width();
    let bw_left = style.border_left_width();
    let has_border = bw_top > 0.0 || bw_right > 0.0 || bw_bottom > 0.0 || bw_left > 0.0;

    if has_border {
        let color_top = style.border_top_color();
        let color_right = style.border_right_color();
        let color_bottom = style.border_bottom_color();
        let color_left = style.border_left_color();
        let uniform_width = bw_top == bw_right && bw_top == bw_bottom && bw_top == bw_left;
        let uniform_color = color_top == color_right && color_top == color_bottom && color_top == color_left;

        if uniform_width && uniform_color && bw_top > 0.0 {
            let border_width = bw_top;
            content.save();
            content.set_stroke_color(color_top.r, color_top.g, color_top.b);
            content.set_line_width(border_width);
            apply_border_style(content, border_style, border_width);

            let offset = border_width / 2.0;
            if radius_tl > 0.0 || radius_tr > 0.0 || radius_br > 0.0 || radius_bl > 0.0 {
                content.rounded_rect_corners(
                    pdf_x + offset,
                    pdf_y + offset,
                    layout.width - border_width,
                    layout.height - border_width,
                    (radius_tl - offset).max(0.0),
                    (radius_tr - offset).max(0.0),
                    (radius_br - offset).max(0.0),
                    (radius_bl - offset).max(0.0),
                );
            } else {
                content.rect(
                    pdf_x + offset,
                    pdf_y + offset,
                    layout.width - border_width,
                    layout.height - border_width,
                );
            }
            content.stroke();
            content.restore();
        } else {
            // Check if non-zero widths are all the same and non-zero colors are all the
            // same.  This is common after pagination splits (one side zeroed, rest equal).
            // In that case draw the full rounded rect clipped to hide the zero-width sides.
            let widths = [bw_top, bw_right, bw_bottom, bw_left];
            let colors = [color_top, color_right, color_bottom, color_left];
            let has_radius =
                radius_tl > 0.0 || radius_tr > 0.0 || radius_br > 0.0 || radius_bl > 0.0;

            let non_zero_w: Vec<f32> = widths.iter().copied().filter(|w| *w > 0.0).collect();
            let non_zero_uniform = !non_zero_w.is_empty()
                && non_zero_w.iter().all(|w| *w == non_zero_w[0]);
            let non_zero_colors: Vec<Color> = widths
                .iter()
                .zip(colors.iter())
                .filter(|(w, _)| **w > 0.0)
                .map(|(_, c)| *c)
                .collect();
            let non_zero_color_uniform = !non_zero_colors.is_empty()
                && non_zero_colors.iter().all(|c| *c == non_zero_colors[0]);

            if has_radius && non_zero_uniform && non_zero_color_uniform {
                let common_width = non_zero_w[0];
                let offset = common_width / 2.0;
                let stroke_color = non_zero_colors[0];

                content.save();
                content.set_stroke_color(stroke_color.r, stroke_color.g, stroke_color.b);
                content.set_line_width(common_width);
                apply_border_style(content, border_style, common_width);

                // Build a clip rect that excludes zero-width sides.  For each side
                // that has width 0, inset the clip by common_width so the rounded-
                // rect stroke on that edge is hidden.  Expand outward on sides that
                // have a border to keep the full stroke visible.
                let margin = common_width;
                let clip_x = if bw_left > 0.0 {
                    pdf_x - margin
                } else {
                    pdf_x + margin
                };
                let clip_right = if bw_right > 0.0 {
                    pdf_x + layout.width + margin
                } else {
                    pdf_x + layout.width - margin
                };
                // PDF y: bottom of element is pdf_y, top is pdf_y + layout.height.
                // CSS "top" border = PDF top = pdf_y + layout.height.
                // CSS "bottom" border = PDF bottom = pdf_y.
                let clip_y = if bw_bottom > 0.0 {
                    pdf_y - margin
                } else {
                    pdf_y + margin
                };
                let clip_top = if bw_top > 0.0 {
                    pdf_y + layout.height + margin
                } else {
                    pdf_y + layout.height - margin
                };

                content.rect(clip_x, clip_y, clip_right - clip_x, clip_top - clip_y);
                content.clip();

                content.rounded_rect_corners(
                    pdf_x + offset,
                    pdf_y + offset,
                    layout.width - common_width,
                    layout.height - common_width,
                    (radius_tl - offset).max(0.0),
                    (radius_tr - offset).max(0.0),
                    (radius_br - offset).max(0.0),
                    (radius_bl - offset).max(0.0),
                );
                content.stroke();
                content.restore();
            } else {
                // Truly non-uniform widths/colors: fall back to straight lines.
                let draw_line = |content: &mut ContentStream,
                                 x1: f32,
                                 y1: f32,
                                 x2: f32,
                                 y2: f32,
                                 width: f32,
                                 color: Color| {
                    content.save();
                    content.set_stroke_color(color.r, color.g, color.b);
                    content.set_line_width(width);
                    apply_border_style(content, border_style, width);
                    content.move_to(x1, y1);
                    content.line_to(x2, y2);
                    content.stroke();
                    content.restore();
                };

                if bw_top > 0.0 {
                    let y = pdf_y + layout.height - (bw_top / 2.0);
                    draw_line(
                        content,
                        pdf_x,
                        y,
                        pdf_x + layout.width,
                        y,
                        bw_top,
                        color_top,
                    );
                }
                if bw_bottom > 0.0 {
                    let y = pdf_y + (bw_bottom / 2.0);
                    draw_line(
                        content,
                        pdf_x,
                        y,
                        pdf_x + layout.width,
                        y,
                        bw_bottom,
                        color_bottom,
                    );
                }
                if bw_left > 0.0 {
                    let x = pdf_x + (bw_left / 2.0);
                    draw_line(
                        content,
                        x,
                        pdf_y,
                        x,
                        pdf_y + layout.height,
                        bw_left,
                        color_left,
                    );
                }
                if bw_right > 0.0 {
                    let x = pdf_x + layout.width - (bw_right / 2.0);
                    draw_line(
                        content,
                        x,
                        pdf_y,
                        x,
                        pdf_y + layout.height,
                        bw_right,
                        color_right,
                    );
                }
            }
        }
    }
}
