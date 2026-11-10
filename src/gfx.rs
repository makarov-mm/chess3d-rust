//! The OpenGL calls the game makes beside its meshes, behind safe functions: the shader program with its
//! uniforms, and the state the renderer switches as it draws. A context must be current on the thread.

use std::ffi::{CStr, CString};

use gl::types::{GLchar, GLenum, GLint, GLuint};

use crate::Result;
use crate::math::{Mat4, Vec3};

type GetParameter = unsafe fn(GLuint, GLenum, *mut GLint);
type GetInfoLog = unsafe fn(GLuint, GLint, *mut GLint, *mut GLchar);

/// What the compiler or the linker had to say about a shader or a program.
fn info_log(object: GLuint, get_parameter: GetParameter, get_info_log: GetInfoLog) -> String {
    let mut length = 0;
    unsafe { get_parameter(object, gl::INFO_LOG_LENGTH, &mut length) };
    let mut log = vec![0u8; length.max(1) as usize];
    let mut written = 0;
    unsafe { get_info_log(object, log.len() as GLint, &mut written, log.as_mut_ptr().cast()) };
    log.truncate(written.clamp(0, log.len() as GLint) as usize);
    String::from_utf8_lossy(&log).trim().to_owned()
}

fn compile(kind: GLenum, stage: &str, source: &str) -> Result<GLuint> {
    let text = CString::new(source).map_err(|_| format!("The {stage} shader has a zero byte in it."))?;
    unsafe {
        let shader = gl::CreateShader(kind);
        gl::ShaderSource(shader, 1, &text.as_ptr(), std::ptr::null());
        gl::CompileShader(shader);
        let mut compiled = 0;
        gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut compiled);
        if compiled == 0 {
            let log = info_log(shader, gl::GetShaderiv, gl::GetShaderInfoLog);
            gl::DeleteShader(shader);
            return Err(format!("The {stage} shader does not compile:\n{log}").into());
        }
        Ok(shader)
    }
}

/// A linked program of a vertex and a fragment shader.
pub struct Program(GLuint);

impl Program {
    pub fn new(vertex_source: &str, fragment_source: &str) -> Result<Program> {
        let vertex = compile(gl::VERTEX_SHADER, "vertex", vertex_source)?;
        let fragment = compile(gl::FRAGMENT_SHADER, "fragment", fragment_source).inspect_err(|_| unsafe { gl::DeleteShader(vertex) })?;
        unsafe {
            let program = gl::CreateProgram();
            gl::AttachShader(program, vertex);
            gl::AttachShader(program, fragment);
            gl::LinkProgram(program);
            gl::DeleteShader(vertex);
            gl::DeleteShader(fragment);
            let mut linked = 0;
            gl::GetProgramiv(program, gl::LINK_STATUS, &mut linked);
            if linked == 0 {
                let log = info_log(program, gl::GetProgramiv, gl::GetProgramInfoLog);
                gl::DeleteProgram(program);
                return Err(format!("The shaders do not link:\n{log}").into());
            }
            Ok(Program(program))
        }
    }

    pub fn bind(&self) {
        unsafe { gl::UseProgram(self.0) };
    }

    /// Where a uniform of the program is set. One the compiler dropped can be set too: OpenGL ignores it.
    pub fn uniform(&self, name: &CStr) -> Uniform {
        Uniform(unsafe { gl::GetUniformLocation(self.0, name.as_ptr()) })
    }
}

impl Drop for Program {
    fn drop(&mut self) {
        unsafe { gl::DeleteProgram(self.0) };
    }
}

/// A uniform of the program in use.
#[derive(Clone, Copy)]
pub struct Uniform(GLint);

impl Uniform {
    pub fn set(self, value: impl UniformValue) {
        value.apply(self.0);
    }
}

pub trait UniformValue {
    fn apply(&self, location: GLint);
}

impl UniformValue for i32 {
    fn apply(&self, location: GLint) {
        unsafe { gl::Uniform1i(location, *self) };
    }
}

impl UniformValue for f32 {
    fn apply(&self, location: GLint) {
        unsafe { gl::Uniform1f(location, *self) };
    }
}

impl UniformValue for Vec3 {
    fn apply(&self, location: GLint) {
        unsafe { gl::Uniform3f(location, self.x, self.y, self.z) };
    }
}

impl UniformValue for Mat4 {
    fn apply(&self, location: GLint) {
        unsafe { gl::UniformMatrix4fv(location, 1, gl::FALSE, self.m.as_ptr()) };
    }
}

pub fn viewport(width: i32, height: i32) {
    unsafe { gl::Viewport(0, 0, width, height) };
}

pub fn clear_color(red: f32, green: f32, blue: f32) {
    unsafe { gl::ClearColor(red, green, blue, 1.0) };
}

/// Clears the color and the depth of the window.
pub fn clear() {
    unsafe { gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT) };
}

pub fn enable(capability: GLenum) {
    unsafe { gl::Enable(capability) };
}

pub fn disable(capability: GLenum) {
    unsafe { gl::Disable(capability) };
}

pub fn depth_func(function: GLenum) {
    unsafe { gl::DepthFunc(function) };
}

pub fn depth_mask(write: bool) {
    unsafe { gl::DepthMask(if write { gl::TRUE } else { gl::FALSE }) };
}

pub fn cull_face(mode: GLenum) {
    unsafe { gl::CullFace(mode) };
}

/// Colors are blended by their alpha. The alpha of the window itself stays at 1 under them: there are systems
/// that show the desktop through a window by it.
pub fn blend_by_alpha() {
    unsafe { gl::BlendFuncSeparate(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA, gl::ONE, gl::ONE_MINUS_SRC_ALPHA) };
}
