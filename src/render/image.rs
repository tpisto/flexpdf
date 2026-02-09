//! Image rendering helpers.

use crate::components::ObjectFit;
use crate::layout::LayoutRect;
use crate::pdf::ContentStream;
use crate::style::ObjectPosition;

pub(super) fn render_image(
    content: &mut ContentStream,
    image_name: &str,
    layout: &LayoutRect,
    page_height: f32,
    img_width: u32,
    img_height: u32,
    object_fit: ObjectFit,
    object_position: Option<ObjectPosition>,
) {

    // Container dimensions (layout box)
    let container_width = layout.width;
    let container_height = layout.height;

    // Image intrinsic dimensions
    let img_w = img_width as f32;
    let img_h = img_height as f32;

    // Calculate the image aspect ratio and container aspect ratio
    let img_aspect = img_w / img_h;
    let container_aspect = container_width / container_height;

    // Container position in PDF coordinates (bottom-left origin, Y flipped)
    let container_x = layout.x;
    let container_y = page_height - layout.y - layout.height;

    // Save graphics state
    content.save();

    let object_position = object_position.unwrap_or_default();
    let pos_x = object_position.x.clamp(0.0, 1.0);
    let pos_y = object_position.y.clamp(0.0, 1.0);

    // Calculate scaled dimensions and position based on object-fit
    let (img_x, img_y, scale_w, scale_h) = match object_fit {
        ObjectFit::Fill => {
            // Stretch to fill the entire container (ignore aspect ratio)
            (container_x, container_y, container_width, container_height)
        }
        ObjectFit::Contain => {
            // Scale to fit inside container while maintaining aspect ratio
            // The image should be fully visible (letterboxing)
            if img_aspect > container_aspect {
                // Image is wider than container - fit by width
                let scale_w = container_width;
                let scale_h = container_width / img_aspect;
                let extra_y = (container_height - scale_h).max(0.0);
                let offset_y = extra_y * pos_y;
                (container_x, container_y + offset_y, scale_w, scale_h)
            } else {
                // Image is taller than container - fit by height
                let scale_h = container_height;
                let scale_w = container_height * img_aspect;
                let extra_x = (container_width - scale_w).max(0.0);
                let offset_x = extra_x * pos_x;
                (container_x + offset_x, container_y, scale_w, scale_h)
            }
        }
        ObjectFit::Cover => {
            // Scale to cover the entire container while maintaining aspect ratio
            // The image may be cropped - set up clipping first
            content.rect(container_x, container_y, container_width, container_height);
            content.clip();

            if img_aspect > container_aspect {
                // Image is wider - fit by height, crop width (center horizontally)
                let scale_h = container_height;
                let scale_w = container_height * img_aspect;
                let extra_x = (scale_w - container_width).max(0.0);
                let offset_x = -extra_x * pos_x;
                (container_x + offset_x, container_y, scale_w, scale_h)
            } else {
                // Image is taller - fit by width, crop height (center vertically)
                let scale_w = container_width;
                let scale_h = container_width / img_aspect;
                let extra_y = (scale_h - container_height).max(0.0);
                let offset_y = -extra_y * pos_y;
                (container_x, container_y + offset_y, scale_w, scale_h)
            }
        }
    };

    // Set up transformation matrix: scale and translate
    // The 'cm' operator takes a transformation matrix [a b c d e f]
    // which transforms coordinates as: x' = ax + cy + e, y' = bx + dy + f
    // For images, we need to scale (width, height) and position (x, y)
    content.transform_matrix(scale_w, 0.0, 0.0, scale_h, img_x, img_y);

    // Draw the image
    content.draw_xobject(image_name);

    // Restore graphics state
    content.restore();
}
