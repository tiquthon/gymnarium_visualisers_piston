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

extern crate gfx_device_gl;
extern crate gymnarium_visualisers_base;
extern crate image;
extern crate piston_window;

use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt::Display;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::thread::JoinHandle;

use gfx_device_gl::Device;

use image::ImageBuffer;

use piston_window::{Context, DrawState, Event, Flip, G2d, G2dTexture, Image, Loop, PistonWindow, Texture, TextureSettings, Window, WindowSettings, EventLoop};

use gymnarium_base::math::{matrix_3x3_as_matrix_3x2, Position2D, Size2D, Transformation2D};
use gymnarium_visualisers_base::input::{
    Button, ButtonArgs, ButtonState, CloseArgs, ControllerAxisArgs, ControllerButton,
    ControllerHat, FileDrag, HatState, Input, Key, Motion, MouseButton, ResizeArgs, Touch,
    TouchArgs,
};
use gymnarium_visualisers_base::{
    Color, Geometry2D, InputProvider, TextureSource, TwoDimensionalDrawableEnvironment,
    TwoDimensionalVisualiser, Viewport2D, Viewport2DModification, Visualiser,
};

/* --- --- --- PistonVisualiserError --- --- --- */

#[derive(Debug)]
pub enum PistonVisualiserError {
    CloseCouldNotJoinRenderThread(String),
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

impl<DrawableEnvironmentError: Error> Display
    for FurtherPistonVisualiserError<DrawableEnvironmentError>
{
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl<DrawableEnvironmentError: Error> Error
    for FurtherPistonVisualiserError<DrawableEnvironmentError>
{
}

impl<DrawableEnvironmentError: Error> From<DrawableEnvironmentError>
    for FurtherPistonVisualiserError<DrawableEnvironmentError>
{
    fn from(error: DrawableEnvironmentError) -> Self {
        Self::RenderingEnvironmentError(error)
    }
}

/* --- --- --- PistonVisualiserInputProvider --- --- --- */

#[derive(Default)]
pub struct PistonVisualiserInputProvider {
    input_queue: Arc<Mutex<VecDeque<Input>>>,
}

impl PistonVisualiserInputProvider {
    fn push_back(&mut self, input: Input) {
        self.input_queue
            .lock()
            .expect("Could not unwrap input_queue in PistonVisualiserInputProvider!")
            .push_back(input);
    }
}

impl InputProvider for PistonVisualiserInputProvider {
    fn clear(&mut self) {
        self.input_queue
            .lock()
            .expect("Could not unwrap input_queue in PistonVisualiserInputProvider!")
            .clear();
    }

    fn peek(&self) -> Option<Input> {
        self.input_queue
            .lock()
            .expect("Could not unwrap input_queue in PistonVisualiserInputProvider!")
            .front()
            .cloned()
    }

    fn pop(&mut self) -> Option<Input> {
        self.input_queue
            .lock()
            .expect("Could not unwrap input_queue in PistonVisualiserInputProvider!")
            .pop_front()
    }

    fn pop_all(&mut self) -> Vec<Input> {
        self.input_queue
            .lock()
            .expect("Could not unwrap input_queue in PistonVisualiserInputProvider!")
            .drain(..)
            .collect()
    }
}

impl Clone for PistonVisualiserInputProvider {
    fn clone(&self) -> Self {
        Self {
            input_queue: Arc::clone(&self.input_queue),
        }
    }
}

/* --- --- --- TextureBuffer --- --- --- */

struct TextureBuffer {
    starting_uses: usize,
    buffered_textures: HashMap<TextureSource, (usize, G2dTexture)>,
}

impl TextureBuffer {
    pub fn new(starting_uses: usize) -> Self {
        Self {
            starting_uses: starting_uses.max(1),
            buffered_textures: HashMap::default(),
        }
    }

    pub fn decrease_and_drop(&mut self) {
        self.buffered_textures
            .iter_mut()
            .for_each(|(_, (counter, _))| {
                if *counter > 0 {
                    (*counter) -= 1;
                }
            });
        let m = self
            .buffered_textures
            .iter()
            .filter(|(_, (counter, _))| *counter == 0)
            .map(|(texture_source, _)| texture_source.clone())
            .collect::<Vec<TextureSource>>();
        m.iter().for_each(|texture_source| {
            let _ = self.buffered_textures.remove(texture_source);
        });
    }

    pub fn load_or_mark_use(&mut self, texture_source: TextureSource, window: &mut PistonWindow) {
        if self.buffered_textures.contains_key(&texture_source) {
            let (counter, _) = self.buffered_textures.get_mut(&texture_source).unwrap();
            (*counter) += 1;
        } else {
            let loaded = match &texture_source {
                TextureSource::Path(path) => Texture::from_path(
                    &mut window.create_texture_context(),
                    path,
                    Flip::None,
                    &TextureSettings::new(),
                )
                .unwrap_or_else(|error| {
                    panic!("Could not load {} as texture (cause: {})", path, error)
                }),
                TextureSource::Bytes {
                    data,
                    width,
                    height,
                } => Texture::from_image(
                    &mut window.create_texture_context(),
                    &ImageBuffer::from_vec(*width, *height, data.clone()).unwrap(),
                    &TextureSettings::new(),
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "Could not load texture from bytes with size {}x{} (cause: {})",
                        width, height, error
                    )
                }),
            };
            let _ = self
                .buffered_textures
                .insert(texture_source, (self.starting_uses, loaded));
        }
    }

    pub fn get(&self, texture_source: &TextureSource) -> Option<&G2dTexture> {
        if let Some((_, texture)) = self.buffered_textures.get(texture_source) {
            Some(texture)
        } else {
            None
        }
    }
}

/* --- --- --- PistonVisualiser --- --- --- */

type PistonVisualiserSyncedData = (
    Vec<Geometry2D>,
    Option<(Viewport2D, Viewport2DModification)>,
    Option<Color>,
);

pub struct PistonVisualiser {
    join_handle: Option<JoinHandle<()>>,
    close_requested: Arc<AtomicBool>,
    closed: Weak<AtomicBool>,

    input_provider: PistonVisualiserInputProvider,

    last_geometries_2d: Vec<Geometry2D>,
    last_preferred_view: Option<(Viewport2D, Viewport2DModification)>,
    last_preferred_background_color: Option<Color>,

    latest_data: Arc<Mutex<Option<PistonVisualiserSyncedData>>>,
}

impl PistonVisualiser {
    pub fn run(window_title: String, window_dimension: (u32, u32), max_frames_per_second: Option<u64>) -> Self {
        let arc1_close_requested = Arc::new(AtomicBool::new(false));
        let arc2_close_requested = Arc::clone(&arc1_close_requested);

        let arc1_closed = Arc::new(AtomicBool::new(false));
        let arc2_closed = Arc::downgrade(&arc1_closed);

        let arc1_latest_data = Arc::new(Mutex::new(Some((Vec::new(), None, None))));
        let arc2_latest_data = Arc::clone(&arc1_latest_data);

        let input_provider_a = PistonVisualiserInputProvider::default();
        let input_provider_b = input_provider_a.clone();

        Self {
            join_handle: Some(thread::spawn(move || {
                Self::thread_function(
                    window_title,
                    window_dimension,
                    max_frames_per_second,
                    arc1_close_requested,
                    arc1_closed,
                    arc1_latest_data,
                    input_provider_a,
                )
            })),
            close_requested: arc2_close_requested,
            closed: arc2_closed,
            input_provider: input_provider_b,
            last_geometries_2d: Vec::new(),
            last_preferred_view: None,
            last_preferred_background_color: None,
            latest_data: arc2_latest_data,
        }
    }

    pub fn input_provider(&self) -> PistonVisualiserInputProvider {
        self.input_provider.clone()
    }

    fn update_texture_buffer(
        texture_buffer: &mut TextureBuffer,
        geometry_2ds: &[Geometry2D],
        window: &mut PistonWindow,
    ) {
        geometry_2ds.iter().for_each(|geometry| {
            if let Geometry2D::Image { texture_source, .. } = geometry {
                texture_buffer.load_or_mark_use(texture_source.clone(), window);
            }
        });
    }

    fn thread_function(
        window_title: String,
        window_dimension: (u32, u32),
        max_frames_per_second: Option<u64>,
        close_requested: Arc<AtomicBool>,
        closed: Arc<AtomicBool>,
        latest_data: Arc<Mutex<Option<PistonVisualiserSyncedData>>>,
        input_provider: PistonVisualiserInputProvider,
    ) {
        let mut window: PistonWindow = WindowSettings::new(window_title.as_str(), window_dimension)
            .exit_on_esc(true)
            .build()
            .expect("Failed to build PistonWindow!");
        window.set_ups(0);
        if let Some(some_max_frames_per_second) = max_frames_per_second {
            window.set_max_fps(some_max_frames_per_second);
        }

        let (mut geometry_2ds, mut preferred_view, mut background_color) = latest_data
            .lock()
            .expect("Could not lock latest_data!")
            .take()
            .unwrap_or_default();

        let mut input_provider = input_provider;

        let mut texture_buffer = TextureBuffer::new(180);

        while let Some(event) = window.next() {
            match event {
                Event::Loop(Loop::Render(_)) => {
                    Self::update_texture_buffer(
                        &mut texture_buffer,
                        &geometry_2ds,
                        &mut window,
                    );
                    window.draw_2d(&event, |context, graphics, device| {
                        Self::render(
                            &context,
                            graphics,
                            device,
                            &geometry_2ds,
                            &preferred_view,
                            &background_color,
                            &texture_buffer,
                        );
                    });
                    texture_buffer.decrease_and_drop();
                }
                Event::Input(input_args, _) => {
                    input_provider.push_back(Self::map_piston_input_to(&input_args));
                }
                _ => {}
            }
            if close_requested.load(std::sync::atomic::Ordering::Relaxed) {
                window.set_should_close(true);
            } else if let Some((new_geometry_2ds, new_preferred_view, new_background_color)) =
                latest_data
                    .lock()
                    .expect("Could not lock latest_data inside while!")
                    .take()
            {
                geometry_2ds = new_geometry_2ds;
                preferred_view = new_preferred_view;
                background_color = new_background_color;
            }
        }
        closed.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    fn map_piston_input_to(piston_input: &piston_window::Input) -> Input {
        match piston_input {
            piston_window::Input::Button(button_args) => Input::Button(ButtonArgs {
                state: match button_args.state {
                    piston_window::ButtonState::Press => ButtonState::Press,
                    piston_window::ButtonState::Release => ButtonState::Release,
                },
                button: match button_args.button {
                    piston_window::Button::Keyboard(key) => Button::Keyboard(match key {
                        piston_window::Key::Unknown => Key::Unknown,
                        piston_window::Key::Backspace => Key::Backspace,
                        piston_window::Key::Tab => Key::Tab,
                        piston_window::Key::Return => Key::Return,
                        piston_window::Key::Escape => Key::Escape,
                        piston_window::Key::Space => Key::Space,
                        piston_window::Key::Exclaim => Key::Exclaim,
                        piston_window::Key::Quotedbl => Key::Quotedbl,
                        piston_window::Key::Hash => Key::Hash,
                        piston_window::Key::Dollar => Key::Dollar,
                        piston_window::Key::Percent => Key::Percent,
                        piston_window::Key::Ampersand => Key::Ampersand,
                        piston_window::Key::Quote => Key::Quote,
                        piston_window::Key::LeftParen => Key::LeftParen,
                        piston_window::Key::RightParen => Key::RightParen,
                        piston_window::Key::Asterisk => Key::Asterisk,
                        piston_window::Key::Plus => Key::Plus,
                        piston_window::Key::Comma => Key::Comma,
                        piston_window::Key::Minus => Key::Minus,
                        piston_window::Key::Period => Key::Period,
                        piston_window::Key::Slash => Key::Slash,
                        piston_window::Key::D0 => Key::D0,
                        piston_window::Key::D1 => Key::D1,
                        piston_window::Key::D2 => Key::D2,
                        piston_window::Key::D3 => Key::D3,
                        piston_window::Key::D4 => Key::D4,
                        piston_window::Key::D5 => Key::D5,
                        piston_window::Key::D6 => Key::D6,
                        piston_window::Key::D7 => Key::D7,
                        piston_window::Key::D8 => Key::D8,
                        piston_window::Key::D9 => Key::D9,
                        piston_window::Key::Colon => Key::Colon,
                        piston_window::Key::Semicolon => Key::Semicolon,
                        piston_window::Key::Less => Key::Less,
                        piston_window::Key::Equals => Key::Equals,
                        piston_window::Key::Greater => Key::Greater,
                        piston_window::Key::Question => Key::Question,
                        piston_window::Key::At => Key::At,
                        piston_window::Key::LeftBracket => Key::LeftBracket,
                        piston_window::Key::Backslash => Key::Backslash,
                        piston_window::Key::RightBracket => Key::RightBracket,
                        piston_window::Key::Caret => Key::Caret,
                        piston_window::Key::Underscore => Key::Underscore,
                        piston_window::Key::Backquote => Key::Backquote,
                        piston_window::Key::A => Key::A,
                        piston_window::Key::B => Key::B,
                        piston_window::Key::C => Key::C,
                        piston_window::Key::D => Key::D,
                        piston_window::Key::E => Key::E,
                        piston_window::Key::F => Key::F,
                        piston_window::Key::G => Key::G,
                        piston_window::Key::H => Key::H,
                        piston_window::Key::I => Key::I,
                        piston_window::Key::J => Key::J,
                        piston_window::Key::K => Key::K,
                        piston_window::Key::L => Key::L,
                        piston_window::Key::M => Key::M,
                        piston_window::Key::N => Key::N,
                        piston_window::Key::O => Key::O,
                        piston_window::Key::P => Key::P,
                        piston_window::Key::Q => Key::Q,
                        piston_window::Key::R => Key::R,
                        piston_window::Key::S => Key::S,
                        piston_window::Key::T => Key::T,
                        piston_window::Key::U => Key::U,
                        piston_window::Key::V => Key::V,
                        piston_window::Key::W => Key::W,
                        piston_window::Key::X => Key::X,
                        piston_window::Key::Y => Key::Y,
                        piston_window::Key::Z => Key::Z,
                        piston_window::Key::Delete => Key::Delete,
                        piston_window::Key::CapsLock => Key::CapsLock,
                        piston_window::Key::F1 => Key::F1,
                        piston_window::Key::F2 => Key::F2,
                        piston_window::Key::F3 => Key::F3,
                        piston_window::Key::F4 => Key::F4,
                        piston_window::Key::F5 => Key::F5,
                        piston_window::Key::F6 => Key::F6,
                        piston_window::Key::F7 => Key::F7,
                        piston_window::Key::F8 => Key::F8,
                        piston_window::Key::F9 => Key::F9,
                        piston_window::Key::F10 => Key::F10,
                        piston_window::Key::F11 => Key::F11,
                        piston_window::Key::F12 => Key::F12,
                        piston_window::Key::PrintScreen => Key::PrintScreen,
                        piston_window::Key::ScrollLock => Key::ScrollLock,
                        piston_window::Key::Pause => Key::Pause,
                        piston_window::Key::Insert => Key::Insert,
                        piston_window::Key::Home => Key::Home,
                        piston_window::Key::PageUp => Key::PageUp,
                        piston_window::Key::End => Key::End,
                        piston_window::Key::PageDown => Key::PageDown,
                        piston_window::Key::Right => Key::Right,
                        piston_window::Key::Left => Key::Left,
                        piston_window::Key::Down => Key::Down,
                        piston_window::Key::Up => Key::Up,
                        piston_window::Key::NumLockClear => Key::NumLockClear,
                        piston_window::Key::NumPadDivide => Key::NumPadDivide,
                        piston_window::Key::NumPadMultiply => Key::NumPadMultiply,
                        piston_window::Key::NumPadMinus => Key::NumPadMinus,
                        piston_window::Key::NumPadPlus => Key::NumPadPlus,
                        piston_window::Key::NumPadEnter => Key::NumPadEnter,
                        piston_window::Key::NumPad1 => Key::NumPad1,
                        piston_window::Key::NumPad2 => Key::NumPad2,
                        piston_window::Key::NumPad3 => Key::NumPad3,
                        piston_window::Key::NumPad4 => Key::NumPad4,
                        piston_window::Key::NumPad5 => Key::NumPad5,
                        piston_window::Key::NumPad6 => Key::NumPad6,
                        piston_window::Key::NumPad7 => Key::NumPad7,
                        piston_window::Key::NumPad8 => Key::NumPad8,
                        piston_window::Key::NumPad9 => Key::NumPad9,
                        piston_window::Key::NumPad0 => Key::NumPad0,
                        piston_window::Key::NumPadPeriod => Key::NumPadPeriod,
                        piston_window::Key::Application => Key::Application,
                        piston_window::Key::Power => Key::Power,
                        piston_window::Key::NumPadEquals => Key::NumPadEquals,
                        piston_window::Key::F13 => Key::F13,
                        piston_window::Key::F14 => Key::F14,
                        piston_window::Key::F15 => Key::F15,
                        piston_window::Key::F16 => Key::F16,
                        piston_window::Key::F17 => Key::F17,
                        piston_window::Key::F18 => Key::F18,
                        piston_window::Key::F19 => Key::F19,
                        piston_window::Key::F20 => Key::F20,
                        piston_window::Key::F21 => Key::F21,
                        piston_window::Key::F22 => Key::F22,
                        piston_window::Key::F23 => Key::F23,
                        piston_window::Key::F24 => Key::F24,
                        piston_window::Key::Execute => Key::Execute,
                        piston_window::Key::Help => Key::Help,
                        piston_window::Key::Menu => Key::Menu,
                        piston_window::Key::Select => Key::Select,
                        piston_window::Key::Stop => Key::Stop,
                        piston_window::Key::Again => Key::Again,
                        piston_window::Key::Undo => Key::Undo,
                        piston_window::Key::Cut => Key::Cut,
                        piston_window::Key::Copy => Key::Copy,
                        piston_window::Key::Paste => Key::Paste,
                        piston_window::Key::Find => Key::Find,
                        piston_window::Key::Mute => Key::Mute,
                        piston_window::Key::VolumeUp => Key::VolumeUp,
                        piston_window::Key::VolumeDown => Key::VolumeDown,
                        piston_window::Key::NumPadComma => Key::NumPadComma,
                        piston_window::Key::NumPadEqualsAS400 => Key::NumPadEqualsAS400,
                        piston_window::Key::AltErase => Key::AltErase,
                        piston_window::Key::Sysreq => Key::Sysreq,
                        piston_window::Key::Cancel => Key::Cancel,
                        piston_window::Key::Clear => Key::Clear,
                        piston_window::Key::Prior => Key::Prior,
                        piston_window::Key::Return2 => Key::Return2,
                        piston_window::Key::Separator => Key::Separator,
                        piston_window::Key::Out => Key::Out,
                        piston_window::Key::Oper => Key::Oper,
                        piston_window::Key::ClearAgain => Key::ClearAgain,
                        piston_window::Key::CrSel => Key::CrSel,
                        piston_window::Key::ExSel => Key::ExSel,
                        piston_window::Key::NumPad00 => Key::NumPad00,
                        piston_window::Key::NumPad000 => Key::NumPad000,
                        piston_window::Key::ThousandsSeparator => Key::ThousandsSeparator,
                        piston_window::Key::DecimalSeparator => Key::DecimalSeparator,
                        piston_window::Key::CurrencyUnit => Key::CurrencyUnit,
                        piston_window::Key::CurrencySubUnit => Key::CurrencySubUnit,
                        piston_window::Key::NumPadLeftParen => Key::NumPadLeftParen,
                        piston_window::Key::NumPadRightParen => Key::NumPadRightParen,
                        piston_window::Key::NumPadLeftBrace => Key::NumPadLeftBrace,
                        piston_window::Key::NumPadRightBrace => Key::NumPadRightBrace,
                        piston_window::Key::NumPadTab => Key::NumPadTab,
                        piston_window::Key::NumPadBackspace => Key::NumPadBackspace,
                        piston_window::Key::NumPadA => Key::NumPadA,
                        piston_window::Key::NumPadB => Key::NumPadB,
                        piston_window::Key::NumPadC => Key::NumPadC,
                        piston_window::Key::NumPadD => Key::NumPadD,
                        piston_window::Key::NumPadE => Key::NumPadE,
                        piston_window::Key::NumPadF => Key::NumPadF,
                        piston_window::Key::NumPadXor => Key::NumPadXor,
                        piston_window::Key::NumPadPower => Key::NumPadPower,
                        piston_window::Key::NumPadPercent => Key::NumPadPercent,
                        piston_window::Key::NumPadLess => Key::NumPadLess,
                        piston_window::Key::NumPadGreater => Key::NumPadGreater,
                        piston_window::Key::NumPadAmpersand => Key::NumPadAmpersand,
                        piston_window::Key::NumPadDblAmpersand => Key::NumPadDblAmpersand,
                        piston_window::Key::NumPadVerticalBar => Key::NumPadVerticalBar,
                        piston_window::Key::NumPadDblVerticalBar => Key::NumPadDblVerticalBar,
                        piston_window::Key::NumPadColon => Key::NumPadColon,
                        piston_window::Key::NumPadHash => Key::NumPadHash,
                        piston_window::Key::NumPadSpace => Key::NumPadSpace,
                        piston_window::Key::NumPadAt => Key::NumPadAt,
                        piston_window::Key::NumPadExclam => Key::NumPadExclam,
                        piston_window::Key::NumPadMemStore => Key::NumPadMemStore,
                        piston_window::Key::NumPadMemRecall => Key::NumPadMemRecall,
                        piston_window::Key::NumPadMemClear => Key::NumPadMemClear,
                        piston_window::Key::NumPadMemAdd => Key::NumPadMemAdd,
                        piston_window::Key::NumPadMemSubtract => Key::NumPadMemSubtract,
                        piston_window::Key::NumPadMemMultiply => Key::NumPadMemMultiply,
                        piston_window::Key::NumPadMemDivide => Key::NumPadMemDivide,
                        piston_window::Key::NumPadPlusMinus => Key::NumPadPlusMinus,
                        piston_window::Key::NumPadClear => Key::NumPadClear,
                        piston_window::Key::NumPadClearEntry => Key::NumPadClearEntry,
                        piston_window::Key::NumPadBinary => Key::NumPadBinary,
                        piston_window::Key::NumPadOctal => Key::NumPadOctal,
                        piston_window::Key::NumPadDecimal => Key::NumPadDecimal,
                        piston_window::Key::NumPadHexadecimal => Key::NumPadHexadecimal,
                        piston_window::Key::LCtrl => Key::LCtrl,
                        piston_window::Key::LShift => Key::LShift,
                        piston_window::Key::LAlt => Key::LAlt,
                        piston_window::Key::LGui => Key::LGui,
                        piston_window::Key::RCtrl => Key::RCtrl,
                        piston_window::Key::RShift => Key::RShift,
                        piston_window::Key::RAlt => Key::RAlt,
                        piston_window::Key::RGui => Key::RGui,
                        piston_window::Key::Mode => Key::Mode,
                        piston_window::Key::AudioNext => Key::AudioNext,
                        piston_window::Key::AudioPrev => Key::AudioPrev,
                        piston_window::Key::AudioStop => Key::AudioStop,
                        piston_window::Key::AudioPlay => Key::AudioPlay,
                        piston_window::Key::AudioMute => Key::AudioMute,
                        piston_window::Key::MediaSelect => Key::MediaSelect,
                        piston_window::Key::Www => Key::Www,
                        piston_window::Key::Mail => Key::Mail,
                        piston_window::Key::Calculator => Key::Calculator,
                        piston_window::Key::Computer => Key::Computer,
                        piston_window::Key::AcSearch => Key::AcSearch,
                        piston_window::Key::AcHome => Key::AcHome,
                        piston_window::Key::AcBack => Key::AcBack,
                        piston_window::Key::AcForward => Key::AcForward,
                        piston_window::Key::AcStop => Key::AcStop,
                        piston_window::Key::AcRefresh => Key::AcRefresh,
                        piston_window::Key::AcBookmarks => Key::AcBookmarks,
                        piston_window::Key::BrightnessDown => Key::BrightnessDown,
                        piston_window::Key::BrightnessUp => Key::BrightnessUp,
                        piston_window::Key::DisplaySwitch => Key::DisplaySwitch,
                        piston_window::Key::KbdIllumToggle => Key::KbdIllumToggle,
                        piston_window::Key::KbdIllumDown => Key::KbdIllumDown,
                        piston_window::Key::KbdIllumUp => Key::KbdIllumUp,
                        piston_window::Key::Eject => Key::Eject,
                        piston_window::Key::Sleep => Key::Sleep,
                    }),
                    piston_window::Button::Mouse(mouse_button) => {
                        Button::Mouse(match mouse_button {
                            piston_window::MouseButton::Unknown => MouseButton::Unknown,
                            piston_window::MouseButton::Left => MouseButton::Left,
                            piston_window::MouseButton::Right => MouseButton::Right,
                            piston_window::MouseButton::Middle => MouseButton::Middle,
                            piston_window::MouseButton::X1 => MouseButton::X1,
                            piston_window::MouseButton::X2 => MouseButton::X2,
                            piston_window::MouseButton::Button6 => MouseButton::Button6,
                            piston_window::MouseButton::Button7 => MouseButton::Button7,
                            piston_window::MouseButton::Button8 => MouseButton::Button8,
                        })
                    }
                    piston_window::Button::Controller(controller_button) => {
                        Button::Controller(ControllerButton {
                            id: controller_button.id,
                            button: controller_button.button,
                        })
                    }
                    piston_window::Button::Hat(controller_hat) => Button::Hat(ControllerHat {
                        id: controller_hat.id,
                        state: match controller_hat.state {
                            piston_window::HatState::Centered => HatState::Centered,
                            piston_window::HatState::Up => HatState::Up,
                            piston_window::HatState::Right => HatState::Right,
                            piston_window::HatState::Down => HatState::Down,
                            piston_window::HatState::Left => HatState::Left,
                            piston_window::HatState::RightUp => HatState::RightUp,
                            piston_window::HatState::RightDown => HatState::RightDown,
                            piston_window::HatState::LeftUp => HatState::LeftUp,
                            piston_window::HatState::LeftDown => HatState::LeftDown,
                        },
                        which: controller_hat.which,
                    }),
                },
                scancode: button_args.scancode,
            }),
            piston_window::Input::Move(motion) => Input::Move(match motion {
                piston_window::Motion::MouseCursor(mouse_cursor) => {
                    Motion::MouseCursor(*mouse_cursor)
                }
                piston_window::Motion::MouseRelative(mouse_relative) => {
                    Motion::MouseRelative(*mouse_relative)
                }
                piston_window::Motion::MouseScroll(mouse_scroll) => {
                    Motion::MouseScroll(*mouse_scroll)
                }
                piston_window::Motion::ControllerAxis(controller_axis) => {
                    Motion::ControllerAxis(ControllerAxisArgs {
                        id: controller_axis.id,
                        axis: controller_axis.axis,
                        position: controller_axis.position,
                    })
                }
                piston_window::Motion::Touch(touch) => Motion::Touch(TouchArgs {
                    device: touch.device,
                    id: touch.id,
                    position_3d: touch.position_3d,
                    pressure_3d: touch.pressure_3d,
                    is_3d: touch.is_3d,
                    touch: match touch.touch {
                        piston_window::Touch::Start => Touch::Start,
                        piston_window::Touch::Move => Touch::Move,
                        piston_window::Touch::End => Touch::End,
                        piston_window::Touch::Cancel => Touch::Cancel,
                    },
                }),
            }),
            piston_window::Input::Text(string) => Input::Text(string.clone()),
            piston_window::Input::Resize(resize_args) => Input::Resize(ResizeArgs {
                window_size: resize_args.window_size,
                draw_size: resize_args.draw_size,
            }),
            piston_window::Input::Focus(focus) => Input::Focus(*focus),
            piston_window::Input::Cursor(cursor) => Input::Cursor(*cursor),
            piston_window::Input::FileDrag(file_drag) => Input::FileDrag(match file_drag {
                piston_window::FileDrag::Hover(hover) => FileDrag::Hover(hover.clone()),
                piston_window::FileDrag::Drop(drop) => FileDrag::Drop(drop.clone()),
                piston_window::FileDrag::Cancel => FileDrag::Cancel,
            }),
            piston_window::Input::Close(_) => Input::Close(CloseArgs {}),
        }
    }

    fn render(
        context: &Context,
        graphics: &mut G2d,
        device: &mut Device,
        geometry_2ds: &[Geometry2D],
        preferred_view: &Option<(Viewport2D, Viewport2DModification)>,
        background_color: &Option<Color>,
        texture_buffer: &TextureBuffer,
    ) {
        if let Some(c) = background_color {
            piston_window::clear(c.float_array(), graphics);
        }

        let (draw_state, transform) = if let Some((viewport, viewport_mod)) = preferred_view {
            match viewport_mod {
                Viewport2DModification::LooseAspectRatio => (
                    piston_window::DrawState::default(),
                    Transformation2D::identity(),
                ),
                Viewport2DModification::KeepAspectRatio
                | Viewport2DModification::KeepAspectRatioAndScissorRemains => {
                    let ctx_vp_rect = context.viewport.unwrap().rect;

                    let mut h = ctx_vp_rect[3] as f64;
                    let mut w = viewport.size.width / viewport.size.height * h;
                    if w > ctx_vp_rect[2] as f64 {
                        w = ctx_vp_rect[2] as f64;
                        h = viewport.size.height / viewport.size.width * w;
                    }

                    let t = Transformation2D::composition(
                        "KeepAspectRatio".to_string(),
                        vec![
                            Transformation2D::translation(
                                Self::window_viewport()
                                    .center
                                    .vector_to(&Position2D::zero()),
                            ),
                            Transformation2D::scale(
                                w / ctx_vp_rect[2] as f64,
                                h / ctx_vp_rect[3] as f64,
                            ),
                            Transformation2D::translation(
                                Position2D::zero().vector_to(&Self::window_viewport().center),
                            ),
                        ],
                    );

                    let draw_state = if *viewport_mod
                        == Viewport2DModification::KeepAspectRatioAndScissorRemains
                    {
                        piston_window::DrawState::default().scissor([
                            (((ctx_vp_rect[2] as f64) - w) / 2f64) as u32,
                            (((ctx_vp_rect[3] as f64) - h) / 2f64) as u32,
                            w as u32,
                            h as u32,
                        ])
                    } else {
                        piston_window::DrawState::default()
                    };

                    (draw_state, t)
                }
            }
        } else {
            (
                piston_window::DrawState::default(),
                Transformation2D::identity(),
            )
        };

        for geometry_2d in geometry_2ds {
            Self::render_geometry_2d(
                context,
                graphics,
                device,
                &draw_state,
                &geometry_2d.clone().append_transformation(transform.clone()),
                texture_buffer,
            );
        }
    }

    fn render_geometry_2d(
        context: &Context,
        graphics: &mut G2d,
        device: &mut Device,
        draw_state: &DrawState,
        geometry_2d: &Geometry2D,
        texture_buffer: &TextureBuffer,
    ) {
        match geometry_2d {
            Geometry2D::Point {
                position,
                color,
                transformations,
            } => {
                let transformed_position = position.transform(transformations);
                let s = context.viewport.unwrap().draw_size;
                piston_window::ellipse::Ellipse::new(color.float_array()).draw(
                    [
                        (transformed_position.x + 1f64) / 2f64 * s[0] as f64,
                        (transformed_position.y + 1f64) / 2f64 * s[1] as f64,
                        1f64,
                        1f64,
                    ],
                    &piston_window::DrawState::default(), // draw_state,
                    context.transform,
                    graphics,
                );
            }
            Geometry2D::Line {
                points,
                line_color,
                line_width,
                line_shape,
                transformations,
            } => {
                piston_window::line::Line::new(line_color.float_array(), *line_width)
                    .shape(match line_shape {
                        gymnarium_visualisers_base::LineShape::Square => {
                            piston_window::line::Shape::Square
                        }
                        gymnarium_visualisers_base::LineShape::Round => {
                            piston_window::line::Shape::Round
                        }
                        gymnarium_visualisers_base::LineShape::Bevel => {
                            piston_window::line::Shape::Bevel
                        }
                    })
                    .draw_from_to(
                        [points[0].x, points[0].y],
                        [points[1].x, points[1].y],
                        draw_state,
                        matrix_3x3_as_matrix_3x2(transformations.transformation_matrix()),
                        graphics,
                    );
            }
            Geometry2D::Polyline {
                points,
                line_color,
                line_width,
                line_shape,
                transformations,
            } => {
                for index in 0..(points.len() - 1) {
                    piston_window::line::Line::new(line_color.float_array(), *line_width)
                        .shape(match line_shape {
                            gymnarium_visualisers_base::LineShape::Square => {
                                piston_window::line::Shape::Square
                            }
                            gymnarium_visualisers_base::LineShape::Round => {
                                piston_window::line::Shape::Round
                            }
                            gymnarium_visualisers_base::LineShape::Bevel => {
                                piston_window::line::Shape::Bevel
                            }
                        })
                        .draw_from_to(
                            [points[index].x, points[index].y],
                            [points[index + 1].x, points[index + 1].y],
                            draw_state,
                            matrix_3x3_as_matrix_3x2(transformations.transformation_matrix()),
                            graphics,
                        );
                }
            }
            Geometry2D::Triangle {
                points,
                fill_color,
                border_color,
                border_width,
                transformations,
            } => {
                let polygon = [
                    [points[0].x, points[0].y],
                    [points[1].x, points[1].y],
                    [points[2].x, points[2].y],
                ];
                piston_window::polygon::Polygon::new(fill_color.float_array()).draw(
                    &polygon,
                    draw_state,
                    matrix_3x3_as_matrix_3x2(transformations.transformation_matrix()),
                    graphics,
                );
                Self::draw_polygon_border(
                    &polygon,
                    border_color.float_array(),
                    *border_width,
                    draw_state,
                    graphics,
                    matrix_3x3_as_matrix_3x2(transformations.transformation_matrix()),
                );
            }
            Geometry2D::Square {
                center_position,
                edge_length,
                fill_color,
                border_color,
                border_width,
                corner_shape,
                transformations,
            } => piston_window::rectangle::Rectangle::new(fill_color.float_array())
                .border(piston_window::rectangle::Border {
                    color: border_color.float_array(),
                    radius: *border_width,
                })
                .shape(match corner_shape {
                    gymnarium_visualisers_base::CornerShape::Square => {
                        piston_window::rectangle::Shape::Square
                    }
                    gymnarium_visualisers_base::CornerShape::Round(size, resolution) => {
                        piston_window::rectangle::Shape::Round(*size, *resolution)
                    }
                    gymnarium_visualisers_base::CornerShape::Bevel(size) => {
                        piston_window::rectangle::Shape::Bevel(*size)
                    }
                })
                .draw(
                    [
                        center_position.x - edge_length / 2f64,
                        center_position.y - edge_length / 2f64,
                        *edge_length,
                        *edge_length,
                    ],
                    draw_state,
                    matrix_3x3_as_matrix_3x2(transformations.transformation_matrix()),
                    graphics,
                ),
            Geometry2D::Rectangle {
                center_position,
                size,
                fill_color,
                border_color,
                border_width,
                corner_shape,
                transformations,
            } => piston_window::rectangle::Rectangle::new(fill_color.float_array())
                .border(piston_window::rectangle::Border {
                    color: border_color.float_array(),
                    radius: *border_width,
                })
                .shape(match corner_shape {
                    gymnarium_visualisers_base::CornerShape::Square => {
                        piston_window::rectangle::Shape::Square
                    }
                    gymnarium_visualisers_base::CornerShape::Round(size, resolution) => {
                        piston_window::rectangle::Shape::Round(*size, *resolution)
                    }
                    gymnarium_visualisers_base::CornerShape::Bevel(size) => {
                        piston_window::rectangle::Shape::Bevel(*size)
                    }
                })
                .draw(
                    [
                        center_position.x - size.width / 2f64,
                        center_position.y - size.height / 2f64,
                        size.width,
                        size.height,
                    ],
                    draw_state,
                    matrix_3x3_as_matrix_3x2(transformations.transformation_matrix()),
                    graphics,
                ),
            Geometry2D::Polygon {
                points,
                fill_color,
                border_color,
                border_width,
                transformations,
            } => {
                // Can draw only non-convex polygons.
                let polygon: Vec<[f64; 2]> = points
                    .iter()
                    .map(|position| [position.x, position.y])
                    .collect();
                piston_window::polygon::Polygon::new(fill_color.float_array()).draw(
                    &polygon,
                    draw_state,
                    matrix_3x3_as_matrix_3x2(transformations.transformation_matrix()),
                    graphics,
                );
                Self::draw_polygon_border(
                    &polygon,
                    border_color.float_array(),
                    *border_width,
                    draw_state,
                    graphics,
                    matrix_3x3_as_matrix_3x2(transformations.transformation_matrix()),
                );
            }
            Geometry2D::Circle {
                center_position,
                radius,
                fill_color,
                border_color,
                border_width,
                transformations,
            } => {
                piston_window::ellipse::Ellipse::new(fill_color.float_array())
                    .border(piston_window::ellipse::Border {
                        color: border_color.float_array(),
                        radius: *border_width,
                    })
                    .draw(
                        [
                            center_position.x - radius,
                            center_position.y - radius,
                            2f64 * radius,
                            2f64 * radius,
                        ],
                        draw_state,
                        matrix_3x3_as_matrix_3x2(transformations.transformation_matrix()),
                        graphics,
                    );
            }
            Geometry2D::Ellipse {
                center_position,
                size,
                fill_color,
                border_color,
                border_width,
                transformations,
            } => {
                piston_window::ellipse::Ellipse::new(fill_color.float_array())
                    .border(piston_window::ellipse::Border {
                        color: border_color.float_array(),
                        radius: *border_width,
                    })
                    .draw(
                        [
                            center_position.x - size.width,
                            center_position.y - size.height,
                            size.width,
                            size.height,
                        ],
                        draw_state,
                        matrix_3x3_as_matrix_3x2(transformations.transformation_matrix()),
                        graphics,
                    );
            }
            Geometry2D::Image {
                center_position,
                size,
                texture_source,
                source_rectangle,
                fill_color,
                transformations,
            } => {
                Image::new()
                    .rect([
                        center_position.x - size.width / 2f64,
                        center_position.y - size.height / 2f64,
                        size.width,
                        size.height,
                    ])
                    .maybe_color(match fill_color {
                        Some(fc) => Some(fc.float_array()),
                        None => None,
                    })
                    .maybe_src_rect(match source_rectangle {
                        Some((src_pos, src_siz)) => Some([
                            src_pos.x - src_siz.width / 2f64,
                            src_pos.y - src_siz.height / 2f64,
                            src_siz.width,
                            src_siz.height,
                        ]),
                        None => None,
                    })
                    .draw(
                        texture_buffer.get(texture_source).unwrap(),
                        draw_state,
                        matrix_3x3_as_matrix_3x2(transformations.transformation_matrix()),
                        graphics,
                    );
            }
            Geometry2D::Group(geometries) => {
                for geometry in geometries {
                    Self::render_geometry_2d(
                        context,
                        graphics,
                        device,
                        draw_state,
                        geometry,
                        texture_buffer,
                    );
                }
            }
        }
    }

    fn draw_polygon_border(
        points: &[[f64; 2]],
        border_color: [f32; 4],
        border_width: f64,
        draw_state: &piston_window::DrawState,
        graphics: &mut G2d,
        transform: [[f64; 3]; 2],
    ) {
        for index in 0..points.len() {
            piston_window::line::Line::new(border_color, border_width)
                .shape(piston_window::line::Shape::Round)
                .draw_from_to(
                    [
                        points[index % points.len()][0],
                        points[index % points.len()][1],
                    ],
                    [
                        points[(index + 1) % points.len()][0],
                        points[(index + 1) % points.len()][1],
                    ],
                    draw_state,
                    transform,
                    graphics,
                );
        }
    }

    fn window_viewport() -> Viewport2D {
        Viewport2D::with(Position2D::zero(), Size2D::with(2f64, 2f64))
    }
}

impl Visualiser<PistonVisualiserError> for PistonVisualiser {
    fn is_open(&self) -> bool {
        match self.closed.upgrade() {
            Some(is_closed) => !is_closed.load(std::sync::atomic::Ordering::Relaxed),
            None => false,
        }
    }

    fn close(&mut self) -> Result<(), PistonVisualiserError> {
        if let Some(jh) = self.join_handle.take() {
            self.close_requested
                .store(true, std::sync::atomic::Ordering::Relaxed);
            jh.join().map_err(|e| {
                PistonVisualiserError::CloseCouldNotJoinRenderThread(format!("{:?}", e))
            })
        } else {
            Ok(())
        }
    }
}

impl<DrawableEnvironmentError: Error>
    TwoDimensionalVisualiser<
        FurtherPistonVisualiserError<DrawableEnvironmentError>,
        PistonVisualiserError,
        DrawableEnvironmentError,
    > for PistonVisualiser
{
    fn render_two_dimensional<
        DrawableEnvironment: TwoDimensionalDrawableEnvironment<DrawableEnvironmentError>,
    >(
        &mut self,
        drawable_environment: &DrawableEnvironment,
    ) -> Result<(), FurtherPistonVisualiserError<DrawableEnvironmentError>> {
        let new_preferred_view = drawable_environment.preferred_view();

        let pref_viewport = if let Some((pref_viewport, _)) = new_preferred_view {
            pref_viewport
        } else {
            Viewport2D::with(Position2D::zero(), Size2D::with(2f64, 2f64))
        };

        let new_geometries_2d = drawable_environment
            .draw_two_dimensional()?
            .into_iter()
            .map(|geometry| geometry.transform(&pref_viewport, &Self::window_viewport()))
            .collect::<Vec<Geometry2D>>();

        let new_background_color = drawable_environment.preferred_background_color();

        if new_geometries_2d != self.last_geometries_2d
            || new_preferred_view != self.last_preferred_view
            || new_background_color != self.last_preferred_background_color
        {
            let mut locked_latest_data = self.latest_data.lock().map_err(|e| {
                FurtherPistonVisualiserError::LockingFailedInternally(format!("{}", e))
            })?;
            (*locked_latest_data) = Some((
                new_geometries_2d.clone(),
                new_preferred_view,
                new_background_color,
            ));
            self.last_geometries_2d = new_geometries_2d;
            self.last_preferred_view = new_preferred_view;
            self.last_preferred_background_color = new_background_color;
        }
        Ok(())
    }
}
