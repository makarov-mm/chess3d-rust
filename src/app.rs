//! The window: it draws the scene, takes the mouse and the keyboard, and lets the computer move.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::Result;
use crate::camera::Camera;
use crate::chess::{DrawReason, Game, Move, Side, Square, Status};
use crate::platform::GlWindow;
use crate::renderer::{self, Renderer, Scene};

const TITLE: &str = "3D Chess - Rust OpenGL Shader Demo";

// The sizes of the window's inside, in logical pixels: the system scales them to the screen.
const SIZE: LogicalSize<f64> = LogicalSize::new(1200.0, 860.0);
const MIN_SIZE: LogicalSize<f64> = LogicalSize::new(720.0, 520.0);

/// Opens the window and runs the game in it until it closes.
pub fn run() -> Result<()> {
    let event_loop = EventLoop::<()>::with_user_event().build()?;
    let mut app = App::new(event_loop.create_proxy());
    event_loop.run_app(&mut app)?;
    app.failure.map_or(Ok(()), Err)
}

/// The window with what draws in it.
struct View {
    renderer: Renderer, // goes before the window, while the OpenGL context is there
    gl: GlWindow,
}

/// The computer looking for its move on another thread.
struct Thinking {
    stop: Arc<AtomicBool>,
    found: Receiver<Option<Move>>,
}

/// What a key asks for.
enum Command {
    NewGame,
    ToggleComputer,
    SwapSides,
    SetLevel(u32),
    ResetCamera,
    Close,
}

impl Command {
    /// The command of a key, by the letter or the digit it types; a key that types something else, as in a
    /// Cyrillic layout, goes by its place on a QWERTY keyboard.
    fn of(event: &KeyEvent) -> Option<Command> {
        let typed = match &event.logical_key {
            Key::Named(NamedKey::Home) => return Some(Command::ResetCamera),
            Key::Named(NamedKey::Escape) => return Some(Command::Close),
            Key::Named(_) => return None,
            Key::Character(text) => text.chars().next().filter(char::is_ascii_alphanumeric),
            _ => None,
        };
        let by_place = || match event.physical_key {
            PhysicalKey::Code(KeyCode::KeyN) => Some('N'),
            PhysicalKey::Code(KeyCode::KeyC) => Some('C'),
            PhysicalKey::Code(KeyCode::KeyS) => Some('S'),
            PhysicalKey::Code(KeyCode::Digit1) => Some('1'),
            PhysicalKey::Code(KeyCode::Digit2) => Some('2'),
            PhysicalKey::Code(KeyCode::Digit3) => Some('3'),
            _ => None,
        };
        match typed.or_else(by_place)?.to_ascii_uppercase() {
            'N' => Some(Command::NewGame),
            'C' => Some(Command::ToggleComputer),
            'S' => Some(Command::SwapSides),
            '1' => Some(Command::SetLevel(1)),
            '2' => Some(Command::SetLevel(2)),
            '3' => Some(Command::SetLevel(3)),
            _ => None,
        }
    }
}

struct App {
    wake: EventLoopProxy<()>,                    // tells the window that the computer has found its move
    view: Option<View>,                          // once the event loop runs
    failure: Option<Box<dyn std::error::Error>>, // why the window did not open

    game: Game,
    camera: Camera,
    clock: Instant,
    selected: Option<Square>,
    legal_moves: Vec<Move>, // of the selected piece

    vs_computer: bool,
    human_side: Side,
    level: u32,                 // 1 to 3: the computer looks 2 to 4 half-moves ahead
    thinking: Option<Thinking>, // while the computer looks for its move

    pointer: Option<PhysicalPosition<f64>>, // where the mouse was last, in pixels of the window
    mouse_down: bool,
    drag_distance: f32, // logical pixels
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.view.is_some() {
            return;
        }
        match open(event_loop) {
            Ok(view) => {
                self.view = Some(view);
                self.reset_camera();
                self.update_status();
                self.start_computer_move();
                // The window comes up with the scene in it.
                self.draw();
                if let Some(window) = self.window() {
                    window.set_visible(true);
                }
            }
            Err(error) => {
                self.failure = Some(error);
                event_loop.exit();
            }
        }
    }

    fn user_event(&mut self, _: &ActiveEventLoop, (): ()) {
        self.finish_computer_move();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(view) = &self.view {
                    view.gl.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => self.draw(),
            // Another window takes the mouse (Alt+Tab): the drag ends.
            WindowEvent::Focused(false) => self.mouse_down = false,
            WindowEvent::CursorMoved { position, .. } => self.move_mouse(position),
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                self.mouse_down = true;
                self.drag_distance = 0.0;
            }
            WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => self.release_mouse(),
            WindowEvent::MouseWheel { delta, .. } => {
                let notches = match delta {
                    MouseScrollDelta::LineDelta(_, lines) => lines,
                    MouseScrollDelta::PixelDelta(pixels) => pixels.y as f32 / (50.0 * self.ui_scale()),
                };
                self.camera.zoom(notches);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && !event.repeat => {
                if let Some(command) = Command::of(&event) {
                    self.run_command(command, event_loop);
                }
            }
            _ => {}
        }
    }

    // Frames are drawn one after another, at the screen refresh rate: swapping the buffers waits for it.
    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(window) = self.window().filter(|window| window.is_minimized() != Some(true)) {
            window.request_redraw();
        }
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.stop_thinking();
        self.view = None;
    }
}

/// Opens the window, hidden until there is a scene to show in it, and gets ready to draw.
fn open(event_loop: &ActiveEventLoop) -> Result<View> {
    let gl = GlWindow::new(event_loop, window_attributes(event_loop))?;
    let renderer = Renderer::new()?;
    Ok(View { renderer, gl })
}

fn window_attributes(event_loop: &ActiveEventLoop) -> WindowAttributes {
    let attributes = Window::default_attributes().with_title(TITLE).with_inner_size(SIZE).with_min_inner_size(MIN_SIZE).with_visible(false);

    // At a high scale, a small screen may not hold the window: then the window takes what the screen has, less
    // some room for its frame and the task bar, in the middle of it.
    let Some(monitor) = event_loop.primary_monitor().or_else(|| event_loop.available_monitors().next()) else { return attributes };
    let (screen, corner) = (monitor.size(), monitor.position());
    let wanted: PhysicalSize<u32> = SIZE.to_physical(monitor.scale_factor());
    let room = PhysicalSize::new(screen.width * 92 / 100, screen.height * 92 / 100);
    if room.width == 0 || room.height == 0 || (wanted.width <= room.width && wanted.height <= room.height) {
        return attributes;
    }
    let size = PhysicalSize::new(wanted.width.min(room.width), wanted.height.min(room.height));
    let left = corner.x + (screen.width as i32 - size.width as i32) / 2;
    let top = corner.y + (screen.height as i32 - size.height as i32) / 2;
    attributes.with_inner_size(size).with_position(PhysicalPosition::new(left, top))
}

impl App {
    fn new(wake: EventLoopProxy<()>) -> App {
        App {
            wake,
            view: None,
            failure: None,
            game: Game::new(),
            camera: Camera::new(),
            clock: Instant::now(),
            selected: None,
            legal_moves: Vec::new(),
            vs_computer: true,
            human_side: Side::White,
            level: 2,
            thinking: None,
            pointer: None,
            mouse_down: false,
            drag_distance: 0.0,
        }
    }

    fn window(&self) -> Option<&Window> {
        self.view.as_ref().map(|view| &view.gl.window)
    }

    /// Pixels in a logical pixel.
    fn ui_scale(&self) -> f32 {
        self.window().map_or(1.0, |window| window.scale_factor() as f32)
    }

    fn computer_to_move(&self) -> bool {
        self.vs_computer && !self.game.is_over() && self.game.turn() != self.human_side
    }

    fn draw(&self) {
        let Some(view) = &self.view else { return };
        let size = view.gl.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }

        let start = Instant::now();
        let scene =
            Scene { game: &self.game, camera: &self.camera, selected: self.selected, moves: &self.legal_moves, time: self.clock.elapsed().as_secs_f32() };
        view.renderer.draw(size.width as i32, size.height as i32, self.ui_scale(), &scene);
        view.gl.swap_buffers();
        // A frame that took less than 2 ms did not wait for the screen (no vertical sync, or a window the driver
        // does not show), so a short sleep follows it, and the frames do not spin.
        if start.elapsed() < Duration::from_millis(2) {
            thread::sleep(Duration::from_millis(1));
        }
    }

    // Input ----------------------------------------------------------------------------------------

    fn move_mouse(&mut self, position: PhysicalPosition<f64>) {
        let previous = self.pointer.replace(position);
        if let (true, Some(previous)) = (self.mouse_down, previous) {
            let dx = (previous.x - position.x) as f32 / self.ui_scale();
            let dy = (position.y - previous.y) as f32 / self.ui_scale();
            self.drag_distance += dx.abs() + dy.abs();
            if self.drag_distance > 3.0 {
                self.camera.rotate(dx, dy);
            }
        }
    }

    fn release_mouse(&mut self) {
        let was_down = std::mem::take(&mut self.mouse_down);
        let Some(position) = self.pointer.filter(|_| was_down) else { return };
        let (x, y) = (position.x as f32, position.y as f32);
        // A click, not a drag, and not on the overlay
        if self.drag_distance <= 6.0 && !renderer::is_on_overlay(x / self.ui_scale(), y / self.ui_scale()) {
            self.click(x, y);
        }
    }

    fn run_command(&mut self, command: Command, event_loop: &ActiveEventLoop) {
        match command {
            Command::NewGame => self.start_new_game(),
            Command::ToggleComputer => {
                self.vs_computer = !self.vs_computer;
                self.stop_thinking();
                self.update_status();
                self.start_computer_move();
            }
            Command::SwapSides => {
                self.human_side = self.human_side.opposite();
                self.vs_computer = true;
                self.start_new_game();
            }
            // The level applies from the computer's next move.
            Command::SetLevel(level) => {
                self.level = level;
                self.update_status();
            }
            Command::ResetCamera => self.reset_camera(),
            Command::Close => event_loop.exit(),
        }
    }

    // The game -------------------------------------------------------------------------------------

    fn start_new_game(&mut self) {
        self.stop_thinking();
        self.game.new_game();
        self.clear_selection();
        self.reset_camera();
        self.update_status();
        self.start_computer_move();
    }

    fn reset_camera(&mut self) {
        self.camera.reset(if self.vs_computer { self.human_side } else { Side::White });
    }

    /// A click on a point of the window, in pixels.
    fn click(&mut self, x: f32, y: f32) {
        if self.game.is_over() || self.thinking.is_some() || self.computer_to_move() {
            return;
        }
        let Some(size) = self.window().map(Window::inner_size) else { return };
        let hit = self.camera.pick(x, y, size.width as f32, size.height as f32, renderer::BOARD_TOP_Y);
        let Some(square) = hit.and_then(renderer::square_at) else { return };

        // The first move to the square: promotions come queen first, so a pawn promotes to a queen.
        if let Some(chosen) = self.legal_moves.iter().copied().find(|candidate| candidate.to == square) {
            self.make_move(chosen);
        } else if self.selected != Some(square) && self.game.piece_at(square).is_some_and(|piece| piece.side == self.game.turn()) {
            self.selected = Some(square);
            self.legal_moves = self.game.legal_moves_from(square);
        } else {
            self.clear_selection(); // a click on the selected piece, or elsewhere
        }
    }

    fn clear_selection(&mut self) {
        self.selected = None;
        self.legal_moves.clear();
    }

    fn make_move(&mut self, chosen: Move) {
        if self.game.try_make_move(chosen) {
            self.clear_selection();
            self.update_status();
            self.start_computer_move();
        }
    }

    /// The search runs on a copy of the game on a thread of its own, so the window keeps drawing and answering;
    /// with its move found, the thread wakes the window.
    fn start_computer_move(&mut self) {
        if !self.computer_to_move() || self.thinking.is_some() {
            return;
        }

        let stop = Arc::new(AtomicBool::new(false));
        let (sender, found) = mpsc::channel();
        let (mut position, depth, wake) = (self.game.fork(), self.level + 1, self.wake.clone());
        let stopped = Arc::clone(&stop);
        thread::spawn(move || {
            // Nobody may be waiting any more: a search told to stop, or the window closed.
            let _ = sender.send(position.find_best_move(depth, &stopped));
            let _ = wake.send_event(());
        });
        self.thinking = Some(Thinking { stop, found });
        self.update_status();
    }

    /// Plays the move the computer has found. A search stopped by a new game, or by the computer turned off,
    /// wakes the window too, but nobody listens to it: its move is no longer wanted.
    fn finish_computer_move(&mut self) {
        let Some(thinking) = &self.thinking else { return };
        let found = match thinking.found.try_recv() {
            Ok(found) => found,
            Err(TryRecvError::Empty) => return, // an earlier search has woken the window: this one is still on
            Err(TryRecvError::Disconnected) => None,
        };

        self.thinking = None;
        if let Some(best) = found {
            self.game.try_make_move(best);
            self.clear_selection();
        }
        self.update_status();
    }

    fn stop_thinking(&mut self) {
        if let Some(thinking) = self.thinking.take() {
            thinking.stop.store(true, Ordering::Relaxed);
        }
    }

    fn update_status(&mut self) {
        let turn = self.game.turn();
        let mover = turn.name().to_uppercase();
        let winner = turn.opposite().name().to_uppercase();
        let draw = |reason: DrawReason| match reason {
            DrawReason::Repetition => "BY REPETITION",
            DrawReason::FiftyMoves => "BY 50 MOVE RULE",
            DrawReason::InsufficientMaterial => "NO MATERIAL",
        };

        let status = match self.game.status() {
            Status::Checkmate => format!("CHECKMATE {winner} WINS"),
            Status::Stalemate => "STALEMATE DRAW".to_owned(),
            Status::Draw(reason) => format!("DRAW {}", draw(reason)),
            _ if self.thinking.is_some() => "COMPUTER THINKING".to_owned(),
            Status::Check => format!("{mover} TO MOVE  CHECK"),
            Status::Playing => format!("{mover} TO MOVE"),
        };
        let players = if self.vs_computer { format!("VS COMPUTER LEVEL {}", self.level) } else { "TWO PLAYERS".to_owned() };
        let title = match self.game.status() {
            Status::Check => format!("Turn: {}  CHECK", turn.name()),
            Status::Checkmate => format!("CHECKMATE - {winner} WINS"),
            Status::Stalemate => "STALEMATE - DRAW".to_owned(),
            Status::Draw(reason) => format!("DRAW {}", draw(reason)),
            Status::Playing => format!("Turn: {}", turn.name()),
        };

        if let Some(view) = &mut self.view {
            view.renderer.set_status([&status, &players]);
            view.gl.window.set_title(&format!("{TITLE}   |   {title}"));
        }
    }
}
