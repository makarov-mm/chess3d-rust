//! Reads the triangles of a Wavefront OBJ model: positions, normals and texture coordinates.

use crate::Result;
use crate::math::{Vec3, vec3};
use crate::mesh::Vertex;

/// A corner of a face: where its position, its texture coordinates and its normal are in the lists read so far.
#[derive(Clone, Copy)]
struct Corner {
    position: usize,
    uv: Option<usize>,
    normal: Option<usize>,
}

/// Something read from a line of the model, or what the line has wrong with it.
type Read<T> = std::result::Result<T, String>;

/// The triangles of a model from its text; `name` is how a mistake in it is told of.
pub fn load(name: &str, text: &str) -> Result<Vec<Vertex>> {
    let (mut positions, mut normals, mut uvs) = (Vec::new(), Vec::new(), Vec::new());
    let mut vertices = Vec::new();

    for (number, line) in text.lines().enumerate() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let read = match words.as_slice() {
            ["v", x, y, z, ..] => read_vec3(x, y, z).map(|position| positions.push(position)),
            ["vn", x, y, z, ..] => read_vec3(x, y, z).map(|normal| normals.push(normal.normalized())),
            ["vt", u, v, ..] => read_number(u).and_then(|u| read_number(v).map(|v| uvs.push([u, v]))),
            ["f", corners @ ..] if corners.len() >= 3 => {
                let corners: Read<Vec<Corner>> = corners.iter().map(|corner| read_corner(corner, positions.len(), uvs.len(), normals.len())).collect();
                // A polygon is a fan of triangles around its first corner.
                corners.map(|corners| {
                    for pair in corners[1..].windows(2) {
                        add_triangle(&mut vertices, [corners[0], pair[0], pair[1]], &positions, &uvs, &normals);
                    }
                })
            }
            _ => Ok(()), // a comment, or something the game has no use for
        };
        read.map_err(|flaw| format!("{name}, line {}: {flaw}", number + 1))?;
    }

    if vertices.is_empty() {
        return Err(format!("{name} has no triangles.").into());
    }
    Ok(vertices)
}

fn add_triangle(vertices: &mut Vec<Vertex>, corners: [Corner; 3], positions: &[Vec3], uvs: &[[f32; 2]], normals: &[Vec3]) {
    // Corners without a normal get the triangle's.
    let [a, b, c] = corners.map(|corner| positions[corner.position]);
    let cross = (b - a).cross(c - a);
    let face_normal = if cross.dot(cross) > 1e-12 { cross.normalized() } else { Vec3::Y };

    vertices.extend(corners.map(|corner| Vertex {
        position: positions[corner.position],
        normal: corner.normal.map_or(face_normal, |index| normals[index]),
        uv: corner.uv.map_or([0.0, 0.0], |index| uvs[index]),
    }));
}

/// "v", "v/vt", "v//vn" or "v/vt/vn", with indices from 1, or negative ones counting back from the end.
fn read_corner(word: &str, position_count: usize, uv_count: usize, normal_count: usize) -> Read<Corner> {
    let mut parts = word.split('/');
    let position = read_index(parts.next().unwrap_or_default(), position_count)?;
    let mut optional = |count: usize| parts.next().filter(|part| !part.is_empty()).map(|part| read_index(part, count)).transpose();
    Ok(Corner { position, uv: optional(uv_count)?, normal: optional(normal_count)? })
}

fn read_index(word: &str, count: usize) -> Read<usize> {
    let index: i64 = word.parse().map_err(|_| format!("\"{word}\" is not a whole number"))?;
    let place = if index > 0 { index - 1 } else { count as i64 + index };
    if index == 0 || place < 0 || place >= count as i64 {
        return Err(format!("index {index} is out of range"));
    }
    Ok(place as usize)
}

fn read_vec3(x: &str, y: &str, z: &str) -> Read<Vec3> {
    Ok(vec3(read_number(x)?, read_number(y)?, read_number(z)?))
}

fn read_number(word: &str) -> Read<f32> {
    word.parse().map_err(|_| format!("\"{word}\" is not a number"))
}
