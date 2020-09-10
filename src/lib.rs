//! # Gymnarium Piston Visualisers
//!
//! `gymnarium_visualisers_piston` contains visualisers and further structures for the
//! `gymnarium_libraries` utilizing the Piston crates.

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

use gymnarium_visualisers_base::{TwoDimensionalVisualiser, TwoDimensionalDrawableEnvironment, Visualiser, Geometry2D, Viewport2D, Position2D, Size2D, Viewport2DModification, Transformation2D};

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

pub struct PistonVisualiser {
    join_handle: Option<JoinHandle<()>>,
    close_requested: Arc<AtomicBool>,
    closed: Arc<AtomicBool>,

    last_geometries_2d: Vec<Geometry2D>,
    latest_geometries_2d: Arc<Mutex<Option<Vec<Geometry2D>>>>,

    last_preferred_view: Option<(Viewport2D, Viewport2DModification)>,
    latest_preferred_view: Arc<Mutex<Option<(Viewport2D, Viewport2DModification)>>>,
}

impl PistonVisualiser {
    pub fn run(
        window_title: String, window_dimension: (u32, u32)
    ) -> Self {
        let arc1_close_requested = Arc::new(AtomicBool::new(false));
        let arc2_close_requested = Arc::clone(&arc1_close_requested);

        let arc1_closed = Arc::new(AtomicBool::new(false));
        let arc2_closed = Arc::clone(&arc1_closed);

        let geometries_2d = Vec::new();
        let arc1_latest_geometries_2d = Arc::new(Mutex::new(Some(geometries_2d.clone())));
        let arc2_latest_geometries_2d = Arc::clone(&arc1_latest_geometries_2d);

        let arc1_latest_preferred_view = Arc::new(Mutex::new(None));
        let arc2_latest_preferred_view = Arc::clone(&arc1_latest_preferred_view);

        Self {
            join_handle: Some(thread::spawn(move || Self::thread_function(
                window_title, window_dimension, arc1_close_requested, arc1_closed,
                arc1_latest_geometries_2d, arc1_latest_preferred_view
            ))),
            close_requested: arc2_close_requested,
            closed: arc2_closed,
            last_geometries_2d: geometries_2d,
            latest_geometries_2d: arc2_latest_geometries_2d,
            last_preferred_view: None,
            latest_preferred_view: arc2_latest_preferred_view,
        }
    }

    fn thread_function(
        window_title: String, window_dimension: (u32, u32), close_requested: Arc<AtomicBool>,
        closed: Arc<AtomicBool>, latest_geometries_2d: Arc<Mutex<Option<Vec<Geometry2D>>>>,
        latest_preferred_view: Arc<Mutex<Option<(Viewport2D, Viewport2DModification)>>>
    ) {
        let mut window: PistonWindow = WindowSettings::new(window_title.as_str(), window_dimension)
            .exit_on_esc(true)
            .build()
            .expect("Failed to build PistonWindow!");

        let mut geometry_2ds = latest_geometries_2d.lock()
            .expect("Could not lock latest_geometries_2d!")
            .take()
            .unwrap_or_default();
        let mut last_preferred_view = latest_preferred_view.lock()
            .expect("Could not lock latest_preferred_view!")
            .take();

        while let Some(event) = window.next() {
            match event {
                Event::Loop(loop_args) => {
                    if let Loop::Render(_) = loop_args {
                        window.draw_2d(&event, |context, graphics, device| {
                            Self::render(&context, graphics, device, &geometry_2ds, &last_preferred_view);
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
            } else {
                geometry_2ds = latest_geometries_2d.lock()
                    .expect("Could not lock latest_geometries_2d inside while!")
                    .take()
                    .unwrap_or(geometry_2ds);
                last_preferred_view = latest_preferred_view.lock()
                    .expect("Could not lock latest_preferred_view inside while!")
                    .take()
                    .or(last_preferred_view);
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
    ) {
        piston_window::clear([1.0; 4], graphics);

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
                // TODO: Output warning
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
            Position2D::with(0f64, 0f64),
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

        let (pref_viewport, pref_viewport_mod) = drawable_environment.preferred_view()?;

        let new_geometries_2d = drawable_environment.draw_two_dimensional()?
            .into_iter()
            .map(|geometry| geometry.transform(
                &pref_viewport,
                &Self::window_viewport(),
            ))
            .collect::<Vec<Geometry2D>>();
        if new_geometries_2d != self.last_geometries_2d {
            let mut locked_latest_geometries_2d = self.latest_geometries_2d.lock()
                .map_err(|e| FurtherPistonVisualiserError::LockingFailedInternally(format!("{}", e)))?;
            (*locked_latest_geometries_2d) = Some(new_geometries_2d.clone());
            self.last_geometries_2d = new_geometries_2d;
        }

        let new_preferred_view = Some((pref_viewport, pref_viewport_mod));
        if new_preferred_view != self.last_preferred_view {
            let mut locked_latest_preferred_view = self.latest_preferred_view.lock()
                .map_err(|e| FurtherPistonVisualiserError::LockingFailedInternally(format!("{}", e)))?;
            (*locked_latest_preferred_view) = new_preferred_view;
            self.last_preferred_view = new_preferred_view;
        }

        Ok(())
    }
}
