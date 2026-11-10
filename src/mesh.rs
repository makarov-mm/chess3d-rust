//! Triangles in a vertex buffer, the meshes made in code, and the pixel font of the overlay.

use std::mem::{offset_of, size_of, size_of_val};

use gl::types::{GLsizei, GLuint};

use crate::math::{Vec3, vec3};

/// A corner of a triangle, as the shaders read it at locations 0, 1 and 2.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: [f32; 2],
}

/// Triangles in a vertex buffer of the current OpenGL context.
pub struct Mesh {
    vertex_array: GLuint,
    buffer: GLuint,
    count: GLsizei,
}

impl Mesh {
    pub fn new(vertices: &[Vertex]) -> Mesh {
        let (mut vertex_array, mut buffer) = (0, 0);
        let stride = size_of::<Vertex>() as GLsizei;
        let attributes = [(3, offset_of!(Vertex, position)), (3, offset_of!(Vertex, normal)), (2, offset_of!(Vertex, uv))];
        unsafe {
            gl::GenVertexArrays(1, &mut vertex_array);
            gl::GenBuffers(1, &mut buffer);
            gl::BindVertexArray(vertex_array);
            gl::BindBuffer(gl::ARRAY_BUFFER, buffer);
            gl::BufferData(gl::ARRAY_BUFFER, size_of_val(vertices) as isize, vertices.as_ptr().cast(), gl::STATIC_DRAW);
            for (index, (size, offset)) in attributes.into_iter().enumerate() {
                gl::EnableVertexAttribArray(index as GLuint);
                gl::VertexAttribPointer(index as GLuint, size, gl::FLOAT, gl::FALSE, stride, offset as *const _);
            }
            gl::BindVertexArray(0);
        }
        Mesh { vertex_array, buffer, count: vertices.len() as GLsizei }
    }

    pub fn draw(&self) {
        unsafe {
            gl::BindVertexArray(self.vertex_array);
            gl::DrawArrays(gl::TRIANGLES, 0, self.count);
        }
    }

    /// The cube from -0.5 to 0.5, with its faces counterclockwise from outside.
    pub fn cube() -> Mesh {
        let mut vertices = Vec::new();
        add_quad(&mut vertices, [vec3(-0.5, -0.5, 0.5), vec3(0.5, -0.5, 0.5), vec3(0.5, 0.5, 0.5), vec3(-0.5, 0.5, 0.5)], Vec3::Z);
        add_quad(&mut vertices, [vec3(0.5, -0.5, -0.5), vec3(-0.5, -0.5, -0.5), vec3(-0.5, 0.5, -0.5), vec3(0.5, 0.5, -0.5)], -Vec3::Z);
        add_quad(&mut vertices, [vec3(0.5, -0.5, 0.5), vec3(0.5, -0.5, -0.5), vec3(0.5, 0.5, -0.5), vec3(0.5, 0.5, 0.5)], Vec3::X);
        add_quad(&mut vertices, [vec3(-0.5, -0.5, -0.5), vec3(-0.5, -0.5, 0.5), vec3(-0.5, 0.5, 0.5), vec3(-0.5, 0.5, -0.5)], -Vec3::X);
        add_quad(&mut vertices, [vec3(-0.5, 0.5, 0.5), vec3(0.5, 0.5, 0.5), vec3(0.5, 0.5, -0.5), vec3(-0.5, 0.5, -0.5)], Vec3::Y);
        add_quad(&mut vertices, [vec3(-0.5, -0.5, -0.5), vec3(0.5, -0.5, -0.5), vec3(0.5, -0.5, 0.5), vec3(-0.5, -0.5, 0.5)], -Vec3::Y);
        Mesh::new(&vertices)
    }

    /// The square from -0.5 to 0.5 in the XZ plane, facing up.
    pub fn plane() -> Mesh {
        let mut vertices = Vec::new();
        add_quad(&mut vertices, [vec3(-0.5, 0.0, 0.5), vec3(0.5, 0.0, 0.5), vec3(0.5, 0.0, -0.5), vec3(-0.5, 0.0, -0.5)], Vec3::Y);
        Mesh::new(&vertices)
    }

    /// A rectangle on the screen, in pixels from the top left corner.
    pub fn screen_rectangle(x: f32, y: f32, width: f32, height: f32) -> Mesh {
        let mut vertices = Vec::new();
        add_screen_rectangle(&mut vertices, x, y, width, height);
        Mesh::new(&vertices)
    }

    /// Lines of text in a font of 5 by 7 pixels, each of them a square of `pixel_size`, from (`x`, `y`) on the
    /// screen down.
    pub fn text<'a>(lines: impl IntoIterator<Item = &'a str>, x: f32, y: f32, pixel_size: f32, line_gap: f32) -> Mesh {
        let mut vertices = Vec::new();
        let mut top = y;
        for line in lines {
            let mut left = x;
            for character in line.chars() {
                let Some(rows) = glyph(character.to_ascii_uppercase()) else {
                    left += 4.0 * pixel_size; // a space, or a character the font lacks
                    continue;
                };
                for (row, pixels) in rows.into_iter().enumerate() {
                    for (column, _) in pixels.bytes().enumerate().filter(|(_, pixel)| *pixel == b'1') {
                        add_screen_rectangle(&mut vertices, left + column as f32 * pixel_size, top + row as f32 * pixel_size, pixel_size, pixel_size);
                    }
                }
                left += 6.0 * pixel_size;
            }
            top += 7.0 * pixel_size + line_gap;
        }
        Mesh::new(&vertices)
    }
}

impl Drop for Mesh {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vertex_array);
            gl::DeleteBuffers(1, &self.buffer);
        }
    }
}

/// Two triangles for the quad with the corners a, b, c and d in turn.
fn add_quad(vertices: &mut Vec<Vertex>, [a, b, c, d]: [Vec3; 4], normal: Vec3) {
    let corners = [(a, [0.0, 0.0]), (b, [1.0, 0.0]), (c, [1.0, 1.0]), (a, [0.0, 0.0]), (c, [1.0, 1.0]), (d, [0.0, 1.0])];
    vertices.extend(corners.map(|(position, uv)| Vertex { position, normal, uv }));
}

fn add_screen_rectangle(vertices: &mut Vec<Vertex>, x: f32, y: f32, width: f32, height: f32) {
    add_quad(vertices, [vec3(x, y, 0.0), vec3(x + width, y, 0.0), vec3(x + width, y + height, 0.0), vec3(x, y + height, 0.0)], Vec3::Z);
}

/// The rows of a character of the font, from the top; a 1 is a pixel drawn.
#[rustfmt::skip]
fn glyph(character: char) -> Option<[&'static str; 7]> {
    Some(match character {
        'A' => ["01110", "10001", "10001", "11111", "10001", "10001", "10001"],
        'B' => ["11110", "10001", "10001", "11110", "10001", "10001", "11110"],
        'C' => ["01111", "10000", "10000", "10000", "10000", "10000", "01111"],
        'D' => ["11110", "10001", "10001", "10001", "10001", "10001", "11110"],
        'E' => ["11111", "10000", "10000", "11110", "10000", "10000", "11111"],
        'F' => ["11111", "10000", "10000", "11110", "10000", "10000", "10000"],
        'G' => ["01111", "10000", "10000", "10011", "10001", "10001", "01111"],
        'H' => ["10001", "10001", "10001", "11111", "10001", "10001", "10001"],
        'I' => ["11111", "00100", "00100", "00100", "00100", "00100", "11111"],
        'J' => ["00111", "00010", "00010", "00010", "10010", "10010", "01100"],
        'K' => ["10001", "10010", "10100", "11000", "10100", "10010", "10001"],
        'L' => ["10000", "10000", "10000", "10000", "10000", "10000", "11111"],
        'M' => ["10001", "11011", "10101", "10101", "10001", "10001", "10001"],
        'N' => ["10001", "11001", "10101", "10011", "10001", "10001", "10001"],
        'O' => ["01110", "10001", "10001", "10001", "10001", "10001", "01110"],
        'P' => ["11110", "10001", "10001", "11110", "10000", "10000", "10000"],
        'Q' => ["01110", "10001", "10001", "10001", "10101", "10010", "01101"],
        'R' => ["11110", "10001", "10001", "11110", "10100", "10010", "10001"],
        'S' => ["01111", "10000", "10000", "01110", "00001", "00001", "11110"],
        'T' => ["11111", "00100", "00100", "00100", "00100", "00100", "00100"],
        'U' => ["10001", "10001", "10001", "10001", "10001", "10001", "01110"],
        'V' => ["10001", "10001", "10001", "10001", "10001", "01010", "00100"],
        'W' => ["10001", "10001", "10001", "10101", "10101", "10101", "01010"],
        'X' => ["10001", "10001", "01010", "00100", "01010", "10001", "10001"],
        'Y' => ["10001", "10001", "01010", "00100", "00100", "00100", "00100"],
        'Z' => ["11111", "00001", "00010", "00100", "01000", "10000", "11111"],
        '0' => ["01110", "10001", "10011", "10101", "11001", "10001", "01110"],
        '1' => ["00100", "01100", "00100", "00100", "00100", "00100", "01110"],
        '2' => ["01110", "10001", "00001", "00010", "00100", "01000", "11111"],
        '3' => ["11110", "00001", "00001", "01110", "00001", "00001", "11110"],
        '4' => ["00010", "00110", "01010", "10010", "11111", "00010", "00010"],
        '5' => ["11111", "10000", "10000", "11110", "00001", "00001", "11110"],
        '6' => ["01110", "10000", "10000", "11110", "10001", "10001", "01110"],
        '7' => ["11111", "00001", "00010", "00100", "01000", "01000", "01000"],
        '8' => ["01110", "10001", "10001", "01110", "10001", "10001", "01110"],
        '9' => ["01110", "10001", "10001", "01111", "00001", "00001", "01110"],
        '+' => ["00000", "00100", "00100", "11111", "00100", "00100", "00000"],
        '/' => ["00001", "00010", "00010", "00100", "01000", "01000", "10000"],
        '-' => ["00000", "00000", "00000", "11111", "00000", "00000", "00000"],
        ':' => ["00000", "00100", "00100", "00000", "00100", "00100", "00000"],
        _ => return None,
    })
}
