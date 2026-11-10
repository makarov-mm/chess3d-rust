//! Draws the scene: the board and the pieces over a glowing floor grid that reflects them, the highlights of
//! the selected piece and its moves, and the text overlay. The OpenGL context must be current.

use crate::Result;
use crate::camera::Camera;
use crate::chess::{Game, Move, PieceKind, Side, Square};
use crate::gfx::{self, Program, Uniform};
use crate::math::{Mat4, PI, Vec3, vec3};
use crate::mesh::Mesh;
use crate::obj;

/// The height of the top of the board.
pub const BOARD_TOP_Y: f32 = 0.055;
const FLOOR_Y: f32 = -0.34;
const PIECE_SCALE: f32 = 0.72;

/// The look of a surface: uMaterial in chess.frag.
#[derive(Clone, Copy)]
enum Material {
    BoardSquare = 0,
    Piece = 1,
    Floor = 2,
    Overlay = 3,
    Highlight = 4,
    BoardBase = 5,
}

const HELP: [&str; 11] = [
    "3D CHESS",
    "",
    "LEFT CLICK SELECT MOVE",
    "DRAG ROTATE   WHEEL ZOOM",
    "",
    "N NEW GAME",
    "C TOGGLE COMPUTER",
    "S SWAP SIDES",
    "1 2 3 LEVEL",
    "HOME RESET CAMERA",
    "ESC QUIT",
];

const LIGHT_SQUARE: Vec3 = vec3(0.78, 0.66, 0.46);
const DARK_SQUARE: Vec3 = vec3(0.18, 0.085, 0.040);
const BOARD_BASE_COLOR: Vec3 = vec3(0.028, 0.025, 0.022);
const WHITE_PIECE: Vec3 = vec3(0.92, 0.86, 0.70);
const BLACK_PIECE: Vec3 = vec3(0.018, 0.024, 0.038);
const FLOOR_COLOR: Vec3 = vec3(0.05, 0.35, 0.85);
const SELECTION_COLOR: Vec3 = vec3(1.0, 0.70, 0.20);
const MOVE_COLOR: Vec3 = vec3(0.06, 0.62, 1.0);

const VERTEX_SHADER: &str = include_str!("../shaders/chess.vert");
const FRAGMENT_SHADER: &str = include_str!("../shaders/chess.frag");

/// The model of a piece: its name, for telling of a mistake in it, and its text.
fn model(kind: PieceKind) -> (&'static str, &'static str) {
    match kind {
        PieceKind::Pawn => ("pawn.obj", include_str!("../models/pawn.obj")),
        PieceKind::Knight => ("knight.obj", include_str!("../models/knight.obj")),
        PieceKind::Bishop => ("bishop.obj", include_str!("../models/bishop.obj")),
        PieceKind::Rook => ("rook.obj", include_str!("../models/rook.obj")),
        PieceKind::Queen => ("queen.obj", include_str!("../models/queen.obj")),
        PieceKind::King => ("king.obj", include_str!("../models/king.obj")),
    }
}

/// What a frame shows.
pub struct Scene<'a> {
    pub game: &'a Game,
    pub camera: &'a Camera,
    pub selected: Option<Square>,
    /// The moves of the selected piece.
    pub moves: &'a [Move],
    /// Seconds, for the animated effects.
    pub time: f32,
}

struct Uniforms {
    model: Uniform,
    view: Uniform,
    projection: Uniform,
    color: Uniform,
    camera_position: Uniform,
    time: Uniform,
    material: Uniform,
    alpha: Uniform,
    reflection: Uniform,
}

pub struct Renderer {
    program: Program,
    uniforms: Uniforms,
    cube: Mesh,
    plane: Mesh,
    pieces: Vec<Mesh>, // by kind
    panel: Mesh,
    help: Mesh,
    status: Option<Mesh>,
    status_lines: [String; 2],
}

// From White's side the a-file is on the left: the files run along -X, the ranks along +Z.
fn square_center(square: Square) -> Vec3 {
    vec3(3.5 - square.file() as f32, BOARD_TOP_Y, square.rank() as f32 - 3.5)
}

/// The square under a point of the board's top.
pub fn square_at(point: Vec3) -> Option<Square> {
    Square::new((4.0 - point.x).floor() as i32, (point.z + 4.0).floor() as i32)
}

/// Whether a point of the window, in logical pixels from its top left corner, is on the overlay: clicks there
/// do not reach the board.
pub fn is_on_overlay(x: f32, y: f32) -> bool {
    (0.0..384.0).contains(&x) && (0.0..320.0).contains(&y)
}

impl Renderer {
    pub fn new() -> Result<Renderer> {
        let program = Program::new(VERTEX_SHADER, FRAGMENT_SHADER)?;
        let uniforms = Uniforms {
            model: program.uniform(c"uModel"),
            view: program.uniform(c"uView"),
            projection: program.uniform(c"uProjection"),
            color: program.uniform(c"uColor"),
            camera_position: program.uniform(c"uCameraPos"),
            time: program.uniform(c"uTime"),
            material: program.uniform(c"uMaterial"),
            alpha: program.uniform(c"uAlpha"),
            reflection: program.uniform(c"uReflection"),
        };

        let load = |kind: PieceKind| -> Result<Mesh> {
            let (name, text) = model(kind);
            Ok(Mesh::new(&obj::load(name, text)?))
        };
        let pieces = PieceKind::ALL.into_iter().map(load).collect::<Result<Vec<Mesh>>>()?;

        gfx::clear_color(0.003, 0.005, 0.010);
        gfx::enable(gl::DEPTH_TEST);
        gfx::depth_func(gl::LEQUAL);
        gfx::cull_face(gl::BACK);
        gfx::enable(gl::BLEND);
        gfx::blend_by_alpha();
        gfx::enable(gl::MULTISAMPLE);

        Ok(Renderer {
            program,
            uniforms,
            cube: Mesh::cube(),
            plane: Mesh::plane(),
            pieces,
            panel: Mesh::screen_rectangle(14.0, 14.0, 360.0, 296.0),
            help: Mesh::text(HELP, 26.0, 28.0, 2.0, 4.0),
            status: None,
            status_lines: Default::default(),
        })
    }

    /// The lines of the status under the help, in a larger font.
    pub fn set_status(&mut self, lines: [&str; 2]) {
        if self.status.is_none() || self.status_lines != lines {
            self.status = Some(Mesh::text(lines, 26.0, 250.0, 3.0, 6.0));
            self.status_lines = lines.map(str::to_owned);
        }
    }

    /// Draws a frame into a window of the given size in pixels; `ui_scale` is how many of them make a logical
    /// pixel of the overlay.
    pub fn draw(&self, width: i32, height: i32, ui_scale: f32, scene: &Scene) {
        gfx::viewport(width, height);
        gfx::clear();
        self.program.bind();

        let eye = scene.camera.eye();
        self.uniforms.view.set(scene.camera.view());
        self.uniforms.projection.set(Camera::projection(width as f32 / height.max(1) as f32));
        self.uniforms.camera_position.set(eye);
        self.uniforms.time.set(scene.time);

        // The reflections first, then the half-transparent floor over them.
        let mirror = Mat4::translation(vec3(0.0, FLOOR_Y * 2.0, 0.0)) * Mat4::scale(vec3(1.0, -1.0, 1.0));
        gfx::depth_mask(false);
        self.draw_pieces(scene.game, mirror, true);
        gfx::depth_mask(true);
        self.draw_floor();

        self.draw_board();
        self.draw_highlights(scene.selected, scene.moves);
        self.draw_pieces(scene.game, Mat4::IDENTITY, false);
        self.draw_overlay(width as f32 / ui_scale, height as f32 / ui_scale);
    }

    fn draw_pieces(&self, game: &Game, world: Mat4, reflection: bool) {
        gfx::disable(gl::CULL_FACE);
        self.uniforms.reflection.set(i32::from(reflection));
        let alpha = if reflection { 0.20 } else { 1.0 };

        for square in Square::all() {
            let Some(piece) = game.piece_at(square) else { continue };
            let center = square_center(square);
            // The knights face the other side.
            let angle = match (piece.kind, piece.side) {
                (PieceKind::Knight, Side::White) => -PI / 2.0,
                (PieceKind::Knight, Side::Black) => PI / 2.0,
                _ => 0.0,
            };
            let model = world
                * Mat4::translation(vec3(center.x, BOARD_TOP_Y + 0.015, center.z))
                * Mat4::rotation_y(angle)
                * Mat4::scale(vec3(PIECE_SCALE, PIECE_SCALE, PIECE_SCALE));
            let color = if piece.side == Side::White { WHITE_PIECE } else { BLACK_PIECE };
            self.draw_mesh(&self.pieces[piece.kind as usize], model, color, Material::Piece, alpha);
        }
        self.uniforms.reflection.set(0);
    }

    fn draw_floor(&self) {
        gfx::disable(gl::CULL_FACE);
        let model = Mat4::translation(vec3(0.0, FLOOR_Y, 0.0)) * Mat4::scale(vec3(18.0, 1.0, 18.0));
        self.draw_mesh(&self.plane, model, FLOOR_COLOR, Material::Floor, 0.76);
    }

    fn draw_board(&self) {
        gfx::enable(gl::CULL_FACE);
        let board_base = Mat4::translation(vec3(0.0, -0.05, 0.0)) * Mat4::scale(vec3(8.90, 0.16, 8.90));
        self.draw_mesh(&self.cube, board_base, BOARD_BASE_COLOR, Material::BoardBase, 1.0);

        for square in Square::all() {
            let center = square_center(square);
            let light = (square.file() + square.rank()) % 2 == 1; // a1 is dark
            let model = Mat4::translation(vec3(center.x, 0.025, center.z)) * Mat4::scale(vec3(0.98, 0.045, 0.98));
            self.draw_mesh(&self.cube, model, if light { LIGHT_SQUARE } else { DARK_SQUARE }, Material::BoardSquare, 1.0);
        }
    }

    fn draw_highlights(&self, selected: Option<Square>, moves: &[Move]) {
        let Some(square) = selected else { return };

        gfx::disable(gl::CULL_FACE);
        gfx::depth_mask(false);
        self.draw_highlight(square, SELECTION_COLOR, 0.55, 0.96);
        let mut previous = None;
        for target in moves.iter().map(|candidate| candidate.to) {
            // The promotions of a pawn come one after another, all to one square: it is marked once.
            if previous.replace(target) != Some(target) {
                self.draw_highlight(target, MOVE_COLOR, 0.33, 0.72);
            }
        }
        gfx::depth_mask(true);
    }

    fn draw_highlight(&self, square: Square, color: Vec3, alpha: f32, size: f32) {
        let center = square_center(square);
        let model = Mat4::translation(vec3(center.x, BOARD_TOP_Y + 0.012, center.z)) * Mat4::scale(vec3(size, 1.0, size));
        self.draw_mesh(&self.plane, model, color, Material::Highlight, alpha);
    }

    /// The help and the status, on a window of the given size in logical pixels.
    fn draw_overlay(&self, width: f32, height: f32) {
        gfx::disable(gl::DEPTH_TEST);
        gfx::disable(gl::CULL_FACE);

        // Logical pixels from the top left corner
        self.uniforms.view.set(Mat4::IDENTITY);
        self.uniforms.projection.set(Mat4::orthographic(0.0, width, height, 0.0, -1.0, 1.0));

        self.draw_mesh(&self.panel, Mat4::IDENTITY, vec3(0.006, 0.011, 0.020), Material::Overlay, 0.58);
        self.draw_mesh(&self.help, Mat4::IDENTITY, vec3(0.72, 0.90, 1.00), Material::Overlay, 0.94);
        if let Some(status) = &self.status {
            self.draw_mesh(status, Mat4::IDENTITY, vec3(1.00, 0.86, 0.42), Material::Overlay, 0.98);
        }

        gfx::enable(gl::DEPTH_TEST);
    }

    fn draw_mesh(&self, mesh: &Mesh, model: Mat4, color: Vec3, material: Material, alpha: f32) {
        self.uniforms.model.set(model);
        self.uniforms.color.set(color);
        self.uniforms.material.set(material as i32);
        self.uniforms.alpha.set(alpha);
        mesh.draw();
    }
}
