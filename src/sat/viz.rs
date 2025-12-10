use super::score::SatelliteScoreFunction;
use super::types::{ExploreTask, Satellite};
use super::utils::{deg_from_e6, haversine_km};
use crate::CBBA;
use crate::consensus::types::BidInfo;
use image::{DynamicImage, Rgba, imageops::FilterType};
use plotters::element::BitMapElement;
use plotters::prelude::*;
use serde::Deserialize;
use std::path::Path;

#[derive(Default, Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct VizConfig {
    pub enable_map: bool,
    pub show_task_info: bool,
    pub show_path_time: bool,
}

pub fn render_visualization(
    filename: &Path,
    caption: &str,
    cbba_instances: &[CBBA<ExploreTask, Satellite, SatelliteScoreFunction>],
    all_tasks: &[ExploreTask],
    options: &VizConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    // Increase resolution to 2K (2560x1440) for better visibility
    let root = BitMapBackend::new(filename, (2560, 1440)).into_drawing_area();
    root.fill(&WHITE)?;

    let mut chart = ChartBuilder::on(&root)
        .caption(caption, ("sans-serif", 50).into_font())
        .margin(20)
        .x_label_area_size(50)
        .y_label_area_size(50)
        .build_cartesian_2d(-180.0..180.0, -90.0..90.0)?;

    // Draw background image if available and requested
    if options.enable_map
        && let Ok(bg_img) = image::open("images/map.png")
    {
        let (w, h) = chart.plotting_area().dim_in_pixel();
        let mut resized_bg = bg_img.resize_exact(w, h, FilterType::Nearest).to_rgba8();

        // Make background semi-transparent/lighter
        for pixel in resized_bg.pixels_mut() {
            let Rgba([r, g, b, a]) = *pixel;
            // Reduce alpha to make it faded
            *pixel = Rgba([r, g, b, (a as f32 * 0.15) as u8]);
        }

        // Map to chart coordinates: -180, 90 is top-left
        chart.draw_series(std::iter::once(BitMapElement::from((
            (-180.0, 90.0),
            DynamicImage::ImageRgba8(resized_bg),
        ))))?;
    }

    chart.configure_mesh().draw()?;

    // Draw tasks as Red Crosses
    chart.draw_series(all_tasks.iter().map(|task| {
        let pos = (deg_from_e6(task.lon_e6), deg_from_e6(task.lat_e6));
        // Increase size of the cross
        EmptyElement::at(pos) + Cross::new((0, 0), 15, RED.filled())
    }))?;

    // Label tasks
    for task in all_tasks {
        chart.draw_series(PointSeries::of_element(
            vec![(deg_from_e6(task.lon_e6), deg_from_e6(task.lat_e6))],
            5,
            &RED.mix(0.0), // Invisible point just for positioning text
            &|c, _s, _st| {
                let allowed_str = if let Some(allowed) = &task.allowed_satellites {
                    let mut allowed_vec: Vec<_> = allowed.iter().collect();
                    allowed_vec.sort();
                    let ids = allowed_vec
                        .iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<_>>()
                        .join(",");
                    format!(" Req:[{}]", ids)
                } else {
                    "".to_string()
                };

                // Combine T{id} and allowed_str
                let label = format!("T{}{}", task.id, allowed_str);

                let label_el = Text::new(
                    label,
                    (8, -8), // Offset to top-right
                    ("sans-serif", 25)
                        .into_font()
                        .style(FontStyle::Bold)
                        .color(&RED),
                );

                let info_str = if options.show_task_info {
                    format!(
                        "S:{:.0} D:{:.3} T:{:.0}s",
                        task.base_score, task.decay_rate_per_hr, task.execution_duration_sec
                    )
                } else {
                    "".to_string()
                };

                let info_el = Text::new(
                    info_str,
                    (8, -28), // Above label
                    ("sans-serif", 15).into_font().color(&RED.mix(0.8)),
                );

                EmptyElement::at(c) + label_el + info_el
            },
        ))?;
    }

    // Draw satellites and assignments
    for cbba in cbba_instances {
        let agent = &cbba.agent;
        let agent_pos = (deg_from_e6(agent.lon_e6), deg_from_e6(agent.lat_e6));

        // Pick a color for this agent
        let color = Palette99::pick(agent.id.0 as usize);

        // Draw satellite as Colored Circle
        chart.draw_series(PointSeries::of_element(
            vec![agent_pos],
            8, // Increase satellite size
            &color.mix(0.8),
            &|c, _s, _st| {
                return EmptyElement::at(c)
                    + Circle::new((0, 0), 8, color.filled())
                    + Text::new(
                        format!("A{}", agent.id),
                        (8, 8), // Offset to bottom-right
                        ("sans-serif", 25)
                            .into_font()
                            .style(FontStyle::Bold)
                            .color(&BLUE),
                    );
            },
        ))?;

        // Draw path assignments
        let path = &cbba.path;
        let mut prev_pos = agent_pos;
        for task in path.iter() {
            let task_pos = (deg_from_e6(task.lon_e6), deg_from_e6(task.lat_e6));

            // Determine if we should wrap around the world
            // If absolute difference in longitude is > 180, it's shorter to go the other way
            let lon_diff = (task_pos.0 - prev_pos.0).abs();

            if lon_diff > 180.0 {
                // Wrap around logic
                // We need to draw two lines:
                // 1. From prev_pos to the edge (e.g. 180 or -180)
                // 2. From the other edge to task_pos

                let (left_pt, right_pt) = if prev_pos.0 < task_pos.0 {
                    (prev_pos, task_pos)
                } else {
                    (task_pos, prev_pos)
                };

                // left_pt is e.g. -170, right_pt is e.g. 170
                // We draw line from left_pt to (-180, y_interp)
                // And line from (180, y_interp) to right_pt

                // Simple interpolation for y at +/- 180
                // slope = dy / dx_wrapped
                // dx_wrapped = (180 - right.x) + (left.x - (-180))

                let _dx_wrapped = (180.0 - right_pt.0) + (left_pt.0 + 180.0);
                let _dy = right_pt.1 - left_pt.1;
                // Careful with direction: left -> right wraps across -180/180
                // Actually, if we go West from Left (-170) to Right (170), we cross -180/180 boundary.

                // Let's simplify:
                // Point A (x1, y1), Point B (x2, y2).
                // If x1 < x2 and we wrap: x1 goes West to -180, wraps to 180, goes to x2.
                // Wait, x1 < x2 usually means East. But if diff > 180, going East is long way.
                // So we go West: x1 -> -180 ... 180 -> x2.

                // If x1 > x2 and we wrap: x1 goes East to 180, wraps to -180, goes to x2.

                let _boundary_y = prev_pos.1 + (task_pos.1 - prev_pos.1) * 0.5; // Rough approx or just use midpoint?
                // Linear interpolation is better
                // Total distance x = (180 - abs(x1)) + (180 - abs(x2))
                // Fraction from P1 to Edge = (180 - abs(x1)) / Total
                // Y_edge = y1 + (y2 - y1) * Fraction

                let dist_to_edge = if prev_pos.0 > 0.0 {
                    180.0 - prev_pos.0
                } else {
                    180.0 + prev_pos.0
                };
                let dist_from_edge = if task_pos.0 > 0.0 {
                    180.0 - task_pos.0
                } else {
                    180.0 + task_pos.0
                };
                let total_x_dist = dist_to_edge + dist_from_edge;

                let fraction = dist_to_edge / total_x_dist;
                let y_edge = prev_pos.1 + (task_pos.1 - prev_pos.1) * fraction;

                let (edge1_x, edge2_x) = if prev_pos.0 > 0.0 {
                    (180.0, -180.0) // East -> West wrapping
                } else {
                    (-180.0, 180.0) // West -> East wrapping
                };

                // If y_edge calculation is wrong, lines will look skewed.
                // But for visual purposes, this is usually sufficient.

                // Draw segment 1
                chart.draw_series(LineSeries::new(
                    vec![prev_pos, (edge1_x, y_edge)],
                    ShapeStyle::from(&color.mix(0.6)).stroke_width(5),
                ))?;

                // Draw segment 2
                chart.draw_series(LineSeries::new(
                    vec![(edge2_x, y_edge), task_pos],
                    ShapeStyle::from(&color.mix(0.6)).stroke_width(5),
                ))?;

                // Draw dashed line connecting edges to show continuity?
                // Optional, but might be helpful.
                /*
                chart.draw_series(LineSeries::new(
                    vec![(edge1_x, y_edge), (edge2_x, y_edge)],
                    ShapeStyle::from(&color.mix(0.3)).stroke_width(1).style(ShapeStyle::Dashed),
                ))?;
                */
            } else {
                // Normal line
                chart.draw_series(LineSeries::new(
                    vec![prev_pos, task_pos],
                    ShapeStyle::from(&color.mix(0.6)).stroke_width(5),
                ))?;
            }

            if options.show_path_time {
                let dist_km = haversine_km(prev_pos.1, prev_pos.0, task_pos.1, task_pos.0);
                let speed_kmps = agent.speed_kmph as f64 / 3600.0;
                let time_sec = dist_km / speed_kmps;

                // Determine label position
                // For wrapped lines, place it near the destination task but slightly offset
                // For normal lines, place it at midpoint
                let label_pos = if lon_diff > 180.0 {
                    // Simple offset from destination for wrapped lines
                    // If dest is positive lon (right), move left. If negative (left), move right.
                    let offset_x = if task_pos.0 > 0.0 { -20.0 } else { 20.0 };
                    (task_pos.0 + offset_x, task_pos.1)
                } else {
                    (
                        (prev_pos.0 + task_pos.0) / 2.0,
                        (prev_pos.1 + task_pos.1) / 2.0,
                    )
                };

                chart.draw_series(PointSeries::of_element(
                    vec![label_pos],
                    0,
                    &BLACK.mix(0.0),
                    &|c, _, _| {
                        return EmptyElement::at(c)
                            + Text::new(
                                format!("{:.0}s", time_sec),
                                (0, 0),
                                ("sans-serif", 15)
                                    .into_font()
                                    .style(FontStyle::Bold)
                                    .color(&color),
                            );
                    },
                ))?;
            }

            prev_pos = task_pos;

            use crate::consensus::types::Task;

            // Draw bid info
            let winners = &cbba.bids;
            if let Some(BidInfo::Winner(_, bid, _)) = winners.get(&task.id()) {
                chart.draw_series(PointSeries::of_element(
                    vec![task_pos],
                    3,
                    &RED.mix(0.0),
                    &|c, _, _| {
                        return EmptyElement::at(c)
                            + Text::new(
                                format!("{:.1}", bid),
                                (8, 45), // Position below the requirement text
                                ("sans-serif", 15).into_font().color(&BLACK.mix(0.8)),
                            );
                    },
                ))?;
            }
        }
    }

    root.present()?;
    Ok(())
}
