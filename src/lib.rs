//! # Gymnarium Piston Visualisers
//!
//! `gymnarium_visualisers_piston` contains visualisers and further structures for the
//! `gymnarium_libraries` utilizing the Piston crates.
//!
//! ## Problems
//!
//! ### Non Convex Polygons
//!
//! This crate is not able to visualise non convex polygons, because I couldn't find something
//! in the piston framework nor in crates.io and I didn't want to implement it myself.

extern crate gymnarium_visualisers_base;
extern crate piston_window;
extern crate gfx_device_gl;

use std::error::Error;
use std::fmt::Display;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;

use piston_window::{WindowSettings, PistonWindow, Event, Loop, Window, Context, G2d, DrawState};

use gfx_device_gl::Device;

use gymnarium_visualisers_base::{
    TwoDimensionalVisualiser, TwoDimensionalDrawableEnvironment, Visualiser, Geometry2D, Viewport2D,
    Position2D, Size2D, Viewport2DModification, Transformation2D, Color
};

/* --- --- --- PistonVisualiserError --- --- --- */

#[derive(Debug)]
pub enum PistonVisualiserError {
    CloseCouldNotJoinRenderThread(String)
}

impl Display for PistonVisualiserError {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl Error for PistonVisualiserError {}

/* --- --- --- PistonVisualiserError --- --- --- */

#[derive(Debug)]
pub enum FurtherPistonVisualiserError<DrawableEnvironmentError: Error> {
    RenderingEnvironmentError(DrawableEnvironmentError),
    LockingFailedInternally(String),
}

impl<DrawableEnvironmentError: Error> Display for FurtherPistonVisualiserError<DrawableEnvironmentError> {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl<DrawableEnvironmentError: Error> Error for FurtherPistonVisualiserError<DrawableEnvironmentError> {}

impl<DrawableEnvironmentError: Error> From<DrawableEnvironmentError> for FurtherPistonVisualiserError<DrawableEnvironmentError> {
    fn from(error: DrawableEnvironmentError) -> Self {
        Self::RenderingEnvironmentError(error)
    }
}

/* --- --- --- PistonVisualiser --- --- --- */

type PistonVisualiserSyncedData = (Vec<Geometry2D>, Option<(Viewport2D, Viewport2DModification)>, Option<Color>);

pub struct PistonVisualiser {
    join_handle: Option<JoinHandle<()>>,
    close_requested: Arc<AtomicBool>,
    closed: Arc<AtomicBool>,

    last_geometries_2d: Vec<Geometry2D>,
    last_preferred_view: Option<(Viewport2D, Viewport2DModification)>,
    last_preferred_background_color: Option<Color>,

    latest_data: Arc<Mutex<Option<PistonVisualiserSyncedData>>>,
}

impl PistonVisualiser {
    pub fn run(
        window_title: String, window_dimension: (u32, u32)
    ) -> Self {
        let arc1_close_requested = Arc::new(AtomicBool::new(false));
        let arc2_close_requested = Arc::clone(&arc1_close_requested);

        let arc1_closed = Arc::new(AtomicBool::new(false));
        let arc2_closed = Arc::clone(&arc1_closed);

        let arc1_latest_data = Arc::new(Mutex::new(Some( (Vec::new(), None, None) )));
        let arc2_latest_data = Arc::clone(&arc1_latest_data);

        Self {
            join_handle: Some(thread::spawn(move || Self::thread_function(
                window_title, window_dimension, arc1_close_requested, arc1_closed,
                arc1_latest_data
            ))),
            close_requested: arc2_close_requested,
            closed: arc2_closed,
            last_geometries_2d: Vec::new(),
            last_preferred_view: None,
            last_preferred_background_color: None,
            latest_data: arc2_latest_data,
        }
    }

    fn thread_function(
        window_title: String, window_dimension: (u32, u32), close_requested: Arc<AtomicBool>,
        closed: Arc<AtomicBool>, latest_data: Arc<Mutex<Option<PistonVisualiserSyncedData>>>
    ) {
        let mut window: PistonWindow = WindowSettings::new(window_title.as_str(), window_dimension)
            .exit_on_esc(true)
            .build()
            .expect("Failed to build PistonWindow!");

        let (mut geometry_2ds, mut preferred_view, mut background_color) = latest_data.lock()
            .expect("Could not lock latest_data!")
            .take()
            .unwrap_or_default();

        while let Some(event) = window.next() {
            match event {
                Event::Loop(loop_args) => {
                    if let Loop::Render(_) = loop_args {
                        window.draw_2d(&event, |context, graphics, device| {
                            Self::render(&context, graphics, device, &geometry_2ds, &preferred_view, &background_color);
                        });
                    }
                },
                Event::Input(_input_args, _) => {
                    // TODO: convert and give back somehow
                },
                _ => {},
            }
            if close_requested.load(std::sync::atomic::Ordering::Relaxed) {
                window.set_should_close(true);
            } else if let Some((new_geometry_2ds, new_preferred_view, new_background_color)) = latest_data.lock()
                .expect("Could not lock latest_data inside while!")
                .take() {
                geometry_2ds = new_geometry_2ds;
                preferred_view = new_preferred_view;
                background_color = new_background_color;
            }
        }
        closed.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    fn render(
        context: &Context,
        graphics: &mut G2d,
        device: &mut Device,
        geometry_2ds: &[Geometry2D],
        preferred_view: &Option<(Viewport2D, Viewport2DModification)>,
        background_color: &Option<Color>,
    ) {
        if let Some(c) = background_color {
            piston_window::clear(c.float_array(), graphics);
        }

        let (draw_state, transform) = if let Some((viewport, viewport_mod)) = preferred_view {
            match viewport_mod {
                Viewport2DModification::LooseAspectRatio => {
                    (piston_window::DrawState::default(), Transformation2D::identity())
                },
                Viewport2DModification::KeepAspectRatio | Viewport2DModification::KeepAspectRatioAndScissorRemains => {

                    let ctx_vp_rect = context.viewport.unwrap().rect;

                    let mut h = ctx_vp_rect[3] as f64;
                    let mut w = viewport.size.width / viewport.size.height * h;
                    if w > ctx_vp_rect[2] as f64 {
                        w = ctx_vp_rect[2] as f64;
                        h = viewport.size.height / viewport.size.width * w;
                    }

                    let t = Transformation2D::composition("KeepAspectRatio".to_string(), vec![
                        Transformation2D::translation(Self::window_viewport().center.vector_to(&Position2D::zero())),
                        Transformation2D::scale(w / ctx_vp_rect[2] as f64, h / ctx_vp_rect[3] as f64),
                        Transformation2D::translation(Position2D::zero().vector_to(&Self::window_viewport().center)),
                    ]);

                    let draw_state = if *viewport_mod == Viewport2DModification::KeepAspectRatioAndScissorRemains {
                        piston_window::DrawState::default()
                            .scissor([
                                (((ctx_vp_rect[2] as f64) - w) / 2f64) as u32,
                                (((ctx_vp_rect[3] as f64) - h) / 2f64) as u32,
                                w as u32,
                                h as u32
                            ])
                    } else {
                        piston_window::DrawState::default()
                    };

                    (draw_state, t)
                },
            }
        } else {
            (piston_window::DrawState::default(), Transformation2D::identity())
        };

        for geometry_2d in geometry_2ds {
            Self::render_geometry_2d(
                context,
                graphics,
                device,
                &draw_state,
                &geometry_2d.clone()
                    .append_transformation(transform.clone())
            );
        }
    }

    fn render_geometry_2d(
        context: &Context,
        graphics: &mut G2d,
        device: &mut Device,
        draw_state: &DrawState,
        geometry_2d: &Geometry2D
    ) {
        match geometry_2d {
            Geometry2D::Point { position, color, transformations } => {
                let transformed_position = position.transform(transformations);
                let s = context.viewport.unwrap().draw_size;
                piston_window::ellipse::Ellipse::new(color.float_array())
                    .draw(
                        [
                            (transformed_position.x + 1f64) / 2f64 * s[0] as f64,
                            (transformed_position.y + 1f64) / 2f64 * s[1] as f64,
                            1f64, 1f64
                        ],
                        &piston_window::DrawState::default(), // draw_state,
                        context.transform,
                        graphics,
                    );
            },
            Geometry2D::Line { points, line_color, line_width, transformations } => {
                piston_window::line::Line::new(line_color.float_array(), *line_width)
                    .shape(piston_window::line::Shape::Round)
                    .draw_from_to(
                        [points[0].x, points[0].y],
                        [points[1].x, points[1].y],
                        draw_state,
                        gymnarium_visualisers_base::matrix_3x3_as_matrix_3x2(
                            transformations.transformation_matrix()
                        ),
                        graphics
                    );
            },
            Geometry2D::Polyline {
                points, line_color, line_width, transformations
            } => {
                for index in 0..(points.len() - 1) {
                    piston_window::line::Line::new(line_color.float_array(), *line_width)
                        .shape(piston_window::line::Shape::Round)
                        .draw_from_to(
                            [points[index].x, points[index].y],
                            [points[index + 1].x, points[index + 1].y],
                            draw_state,
                            gymnarium_visualisers_base::matrix_3x3_as_matrix_3x2(
                                transformations.transformation_matrix()
                            ),
                            graphics
                        );
                }
            },
            Geometry2D::Triangle {
                points, fill_color, border_color, border_width,
                transformations
            } => {
                let polygon = [
                    [points[0].x, points[0].y],
                    [points[1].x, points[1].y],
                    [points[2].x, points[2].y]
                ];
                piston_window::polygon::Polygon::new(fill_color.float_array())
                    .draw(
                        &polygon,
                        draw_state,
                        gymnarium_visualisers_base::matrix_3x3_as_matrix_3x2(
                            transformations.transformation_matrix()
                        ),
                        graphics
                    );
                Self::draw_polygon_border(
                    &polygon, border_color.float_array(), *border_width,
                    draw_state, graphics, gymnarium_visualisers_base::matrix_3x3_as_matrix_3x2(
                        transformations.transformation_matrix()
                    )
                );
            },
            Geometry2D::Square { center_position, edge_length, fill_color, border_color, border_width, transformations } => {
                piston_window::rectangle::Rectangle::new(fill_color.float_array())
                    .border(piston_window::rectangle::Border {
                        color: border_color.float_array(),
                        radius: *border_width
                    })
                    .draw(
                        [
                            center_position.x - edge_length / 2f64,
                            center_position.y - edge_length / 2f64,
                            *edge_length,
                            *edge_length,
                        ],
                        draw_state,
                        gymnarium_visualisers_base::matrix_3x3_as_matrix_3x2(
                            transformations.transformation_matrix()
                        ),
                        graphics
                    )
            },
            Geometry2D::Rectangle {
                center_position, size, fill_color, border_color,
                border_width, transformations
            } => {
                piston_window::rectangle::Rectangle::new(fill_color.float_array())
                    .border(piston_window::rectangle::Border {
                        color: border_color.float_array(),
                        radius: *border_width
                    })
                    .draw(
                        [
                            center_position.x - size.width / 2f64,
                            center_position.y - size.height / 2f64,
                            size.width,
                            size.height,
                        ],
                        draw_state,
                        gymnarium_visualisers_base::matrix_3x3_as_matrix_3x2(
                            transformations.transformation_matrix()
                        ),
                        graphics
                    )
            },
            Geometry2D::Polygon { points, fill_color, border_color, border_width, transformations } => {
                // Can draw only non-convex polygons.
                let polygon: Vec<[f64; 2]> = points.iter()
                    .map(|position| [position.x, position.y])
                    .collect();
                piston_window::polygon::Polygon::new(fill_color.float_array())
                    .draw(
                        &polygon,
                        draw_state,
                        gymnarium_visualisers_base::matrix_3x3_as_matrix_3x2(
                            transformations.transformation_matrix()
                        ),
                        graphics
                    );
                Self::draw_polygon_border(
                    &polygon, border_color.float_array(), *border_width,
                    draw_state, graphics, gymnarium_visualisers_base::matrix_3x3_as_matrix_3x2(
                        transformations.transformation_matrix()
                    )
                );
            },
            Geometry2D::Circle {
                center_position, radius, fill_color, border_color,
                border_width, transformations
            } => {
                piston_window::ellipse::Ellipse::new(fill_color.float_array())
                    .border(piston_window::ellipse::Border {
                        color: border_color.float_array(),
                        radius: *border_width
                    })
                    .draw(
                        [
                            center_position.x - radius,
                            center_position.y - radius,
                            2f64 * radius,
                            2f64 * radius
                        ],
                        draw_state,
                        gymnarium_visualisers_base::matrix_3x3_as_matrix_3x2(
                            transformations.transformation_matrix()
                        ),
                        graphics
                    );
            },
            Geometry2D::Ellipse {
                center_position, size, fill_color, border_color,
                border_width, transformations
            } => {
                piston_window::ellipse::Ellipse::new(fill_color.float_array())
                    .border(piston_window::ellipse::Border {
                        color: border_color.float_array(),
                        radius: *border_width
                    })
                    .draw(
                        [
                            center_position.x - size.width,
                            center_position.y - size.height,
                            size.width,
                            size.height
                        ],
                        draw_state,
                        gymnarium_visualisers_base::matrix_3x3_as_matrix_3x2(
                            transformations.transformation_matrix()
                        ),
                        graphics
                    );
            },
            Geometry2D::Group(geometries) => {
                for geometry in geometries {
                    Self::render_geometry_2d(
                        context,
                        graphics,
                        device,
                        draw_state,
                        geometry
                    );
                }
            },
        }
    }

    fn draw_polygon_border(
        points: &[[f64; 2]], border_color: [f32; 4], border_width: f64,
        draw_state: &piston_window::DrawState, graphics: &mut G2d, transform: [[f64; 3]; 2]
    ) {
        for index in 0..points.len() {
            piston_window::line::Line::new(border_color, border_width)
                .shape(piston_window::line::Shape::Round)
                .draw_from_to(
                    [points[index % points.len()][0], points[index % points.len()][1]],
                    [points[(index + 1) % points.len()][0], points[(index + 1) % points.len()][1]],
                    draw_state,
                    transform,
                    graphics
                );
        }
    }

    fn window_viewport() -> Viewport2D {
        Viewport2D::with(
            Position2D::zero(),
            Size2D::with(2f64, 2f64)
        )
    }
}

impl Visualiser<PistonVisualiserError> for PistonVisualiser {
    fn is_open(&self) -> bool {
        !self.closed.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn close(&mut self) -> Result<(), PistonVisualiserError> {
        if let Some(jh) = self.join_handle.take() {
            self.close_requested.store(true, std::sync::atomic::Ordering::Relaxed);
            jh.join()
                .map_err(|e| PistonVisualiserError::CloseCouldNotJoinRenderThread(format!("{:?}", e)))
        } else {
            Ok(())
        }
    }
}

impl<DrawableEnvironmentError: Error> TwoDimensionalVisualiser<
    FurtherPistonVisualiserError<DrawableEnvironmentError>, PistonVisualiserError, DrawableEnvironmentError
> for PistonVisualiser {
    fn render_two_dimensional<
        DrawableEnvironment: TwoDimensionalDrawableEnvironment<DrawableEnvironmentError>
    >(&mut self, drawable_environment: &DrawableEnvironment) -> Result<(), FurtherPistonVisualiserError<DrawableEnvironmentError>> {

        let new_preferred_view = drawable_environment.preferred_view();

        let pref_viewport = if let Some((pref_viewport, _)) = new_preferred_view {
            pref_viewport
        } else {
            Viewport2D::with(Position2D::zero(), Size2D::with(2f64, 2f64))
        };

        let new_geometries_2d = drawable_environment.draw_two_dimensional()?
            .into_iter()
            .map(|geometry| geometry.transform(
                &pref_viewport,
                &Self::window_viewport(),
            ))
            .collect::<Vec<Geometry2D>>();

        let new_background_color = drawable_environment.preferred_background_color();

        if new_geometries_2d != self.last_geometries_2d || new_preferred_view != self.last_preferred_view || new_background_color != self.last_preferred_background_color {
            let mut locked_latest_data = self.latest_data.lock()
                .map_err(|e| FurtherPistonVisualiserError::LockingFailedInternally(format!("{}", e)))?;
            (*locked_latest_data) = Some((new_geometries_2d.clone(), new_preferred_view, new_background_color));
            self.last_geometries_2d = new_geometries_2d;
            self.last_preferred_view = new_preferred_view;
            self.last_preferred_background_color = new_background_color;
        }
        Ok(())
    }
}
