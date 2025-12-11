use super::score::SatelliteScoreFunction;
use super::types::{ExploreTask, Satellite};
use super::utils::{deg_from_e6, haversine_km};
use crate::CBBA;
use crate::consensus::types::{BidInfo, Task as TaskTrait, TaskId};
use plotters::coord::types::RangedCoordf64;
use plotters::prelude::*;
use plotters::style::PaletteColor;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use image::ImageReader;

type Chart2d<'a> = ChartContext<'a, BitMapBackend<'a>, Cartesian2d<RangedCoordf64, RangedCoordf64>>;
type Coord = (f64, f64);
type AgentColor = PaletteColor<Palette99>;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct VizConfig {
    pub enabled: bool,
    pub output_dir: String,
    pub file_name: String,
    pub image_size: (u32, u32),
    pub enable_map: bool,
    pub show_task_info: bool,
    pub show_path_time: bool,
    pub font_size: f64,
    pub line_thickness: u32,
}

impl Default for VizConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            output_dir: "visualizations".to_string(),
            file_name: "viz.png".to_string(),
            image_size: (1024, 1024),
            enable_map: false,
            show_task_info: true,
            show_path_time: true,
            font_size: 16.0,
            line_thickness: 2,
        }
    }
}
pub fn render_visualization(
    filename: &Path,
    caption: &str,
    cbba_instances: &[CBBA<ExploreTask, Satellite, SatelliteScoreFunction>],
    all_tasks: &[ExploreTask],
    options: &VizConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let (width, height) = options.image_size;
    let buf_size = (width * height * 3) as usize;
    let mut buffer = if options.enable_map {
        let map_path = Path::new("images/map.png");
        let map_image = ImageReader::open(map_path)?.decode()?;
        let resized = image::imageops::resize(
            &map_image.to_rgb8(),
            width,
            height,
            image::imageops::FilterType::Lanczos3,
        );
        resized.into_raw()
    } else {
        vec![255; buf_size]
    };

    {
        let root = BitMapBackend::with_buffer(&mut buffer, (width, height)).into_drawing_area();

        let mut chart: Chart2d<'_> = ChartBuilder::on(&root)
            .caption(caption, ("sans-serif", 50).into_font())
            .margin(20)
            .x_label_area_size(50)
            .y_label_area_size(50)
            .build_cartesian_2d(-180.0..180.0, -90.0..90.0)?;

        chart.configure_mesh().draw()?;
        draw_tasks(&mut chart, all_tasks, options)?;
        draw_agents_and_paths(&mut chart, cbba_instances, options)?;

        root.present()?;
    }

    if let Some(parent) = filename.parent() {
        std::fs::create_dir_all(parent)?;
    }
    image::save_buffer(filename, &buffer, width, height, image::ColorType::Rgb8)?;
    Ok(())
}

fn draw_tasks(
    chart: &mut Chart2d<'_>,
    tasks: &[ExploreTask],
    options: &VizConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    chart.draw_series(tasks.iter().map(|task| {
        let pos = (deg_from_e6(task.lon_e6), deg_from_e6(task.lat_e6));
        EmptyElement::at(pos) + Cross::new((0, 0), 15, RED.filled())
    }))?;

    for task in tasks {
        let point_radius = (options.line_thickness as i32).max(2);
        chart.draw_series(PointSeries::of_element(
            vec![(deg_from_e6(task.lon_e6), deg_from_e6(task.lat_e6))],
            point_radius,
            &RED.mix(0.0),
            &|c, _s, _st| {
                let allowed_str = task
                    .allowed_satellites
                    .as_ref()
                    .map(|allowed| {
                        let mut allowed_vec: Vec<_> = allowed.iter().collect();
                        allowed_vec.sort();
                        let ids = allowed_vec
                            .iter()
                            .map(|id| id.to_string())
                            .collect::<Vec<_>>()
                            .join(",");
                        format!(" Req:[{}]", ids)
                    })
                    .unwrap_or_default();

                let label = format!("T{}{}", task.id, allowed_str);

                let label_el = Text::new(
                    label,
                    (8, -8),
                    ("sans-serif", options.font_size)
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
                    String::new()
                };

                let info_el = Text::new(
                    info_str,
                    (8, -28),
                    ("sans-serif", options.font_size * 0.75)
                        .into_font()
                        .color(&RED.mix(0.8)),
                );

                EmptyElement::at(c) + label_el + info_el
            },
        ))?;
    }

    Ok(())
}

fn draw_agents_and_paths(
    chart: &mut Chart2d<'_>,
    cbba_instances: &[CBBA<ExploreTask, Satellite, SatelliteScoreFunction>],
    options: &VizConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    for cbba in cbba_instances {
        let agent = &cbba.agent;
        let agent_pos = (deg_from_e6(agent.lon_e6), deg_from_e6(agent.lat_e6));
        let color: AgentColor = Palette99::pick(agent.id.0 as usize);

        draw_agent_marker(chart, agent_pos, agent.id.0, &color, options)?;
        draw_path(chart, cbba, agent_pos, &color, options)?;
    }

    Ok(())
}

fn draw_agent_marker(
    chart: &mut Chart2d<'_>,
    pos: Coord,
    agent_id: u32,
    color: &AgentColor,
    options: &VizConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    chart.draw_series(PointSeries::of_element(
        vec![pos],
        (options.line_thickness as i32).max(4),
        &color.mix(0.8),
        &|c, _s, _st| {
            EmptyElement::at(c)
                + Circle::new((0, 0), options.line_thickness as i32 * 3, color.filled())
                + Text::new(
                    format!("A{}", agent_id),
                    (8, 8),
                    ("sans-serif", options.font_size)
                        .into_font()
                        .style(FontStyle::Bold)
                        .color(&BLUE),
                )
        },
    ))?;

    Ok(())
}

fn draw_path(
    chart: &mut Chart2d<'_>,
    cbba: &CBBA<ExploreTask, Satellite, SatelliteScoreFunction>,
    mut prev_pos: Coord,
    color: &AgentColor,
    options: &VizConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    for task in &cbba.path {
        let task_pos = (deg_from_e6(task.lon_e6), deg_from_e6(task.lat_e6));
        let wrapped = (task_pos.0 - prev_pos.0).abs() > 180.0;

        if wrapped {
            draw_wrapped_segment(chart, color, prev_pos, task_pos, options)?;
        } else {
            draw_direct_segment(chart, color, prev_pos, task_pos, options)?;
        }

        if options.show_path_time {
            draw_travel_time(
                chart,
                color,
                cbba.agent.speed_kmph as f64,
                prev_pos,
                task_pos,
                wrapped,
                options,
            )?;
        }

        draw_bid_info(chart, &cbba.bids, task_pos, task.id(), options)?;
        prev_pos = task_pos;
    }

    Ok(())
}

fn draw_direct_segment(
    chart: &mut Chart2d<'_>,
    color: &AgentColor,
    from: Coord,
    to: Coord,
    options: &VizConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    chart.draw_series(LineSeries::new(
        vec![from, to],
        ShapeStyle::from(&color.mix(0.6)).stroke_width(options.line_thickness),
    ))?;
    Ok(())
}

fn draw_wrapped_segment(
    chart: &mut Chart2d<'_>,
    color: &AgentColor,
    from: Coord,
    to: Coord,
    options: &VizConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let dist_to_edge = if from.0 > 0.0 {
        180.0 - from.0
    } else {
        180.0 + from.0
    };
    let dist_from_edge = if to.0 > 0.0 {
        180.0 - to.0
    } else {
        180.0 + to.0
    };
    let total_x_dist = dist_to_edge + dist_from_edge;
    let fraction = dist_to_edge / total_x_dist;
    let y_edge = from.1 + (to.1 - from.1) * fraction;

    let (edge1_x, edge2_x) = if from.0 > 0.0 {
        (180.0, -180.0)
    } else {
        (-180.0, 180.0)
    };

    chart.draw_series(LineSeries::new(
        vec![from, (edge1_x, y_edge)],
        ShapeStyle::from(&color.mix(0.6)).stroke_width(options.line_thickness),
    ))?;

    chart.draw_series(LineSeries::new(
        vec![(edge2_x, y_edge), to],
        ShapeStyle::from(&color.mix(0.6)).stroke_width(options.line_thickness),
    ))?;

    Ok(())
}

fn draw_travel_time(
    chart: &mut Chart2d<'_>,
    color: &AgentColor,
    speed_kmph: f64,
    from: Coord,
    to: Coord,
    wrapped: bool,
    options: &VizConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let dist_km = haversine_km(from.1, from.0, to.1, to.0);
    let speed_kmps = speed_kmph / 3600.0;
    let time_sec = dist_km / speed_kmps;

    let label_pos = if wrapped {
        let offset_x = if to.0 > 0.0 { -20.0 } else { 20.0 };
        (to.0 + offset_x, to.1)
    } else {
        ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0)
    };

    chart.draw_series(PointSeries::of_element(
        vec![label_pos],
        0,
        &BLACK.mix(0.0),
        &|c, _, _| {
            EmptyElement::at(c)
                + Text::new(
                    format!("{:.0}s", time_sec),
                    (0, 0),
                    ("sans-serif", options.font_size * 0.85)
                        .into_font()
                        .style(FontStyle::Bold)
                        .color(color),
                )
        },
    ))?;

    Ok(())
}

fn draw_bid_info(
    chart: &mut Chart2d<'_>,
    bids: &HashMap<TaskId, BidInfo>,
    pos: Coord,
    task_id: TaskId,
    options: &VizConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(BidInfo::Winner(_, bid, _)) = bids.get(&task_id) {
        chart.draw_series(PointSeries::of_element(
            vec![pos],
            3,
            &RED.mix(0.0),
            &|c, _, _| {
                EmptyElement::at(c)
                    + Text::new(
                        format!("{:.1}", bid),
                        (8, 45),
                        ("sans-serif", options.font_size * 0.65)
                            .into_font()
                            .color(&BLACK.mix(0.8)),
                    )
            },
        ))?;
    }

    Ok(())
}
