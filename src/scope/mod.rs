extern crate gdk;

pub mod cairo_scope;

pub struct Style {
  pub foreground_colour: gdk::RGBA,
  pub background_colour: gdk::RGBA,
  pub line_size: f64,
  pub antialias: bool,
  pub reflect: bool,
  pub cursor: Option<usize>,
  pub width: u32,
  pub height: u32,
  pub gain: f64,
}

pub struct Scope {
  pub samples: Vec<f32>,
  pub scope_buffer: Vec<f32>,
  pub read_count: usize,
  pub write_count: usize,
  pub sample_rate: usize,
  pub cursor: bool,
  pub hold: bool,
  pub style: Style,
}

impl Scope {
  pub fn new() -> Scope {
    Scope {
      samples: vec![0.; 8192],
      scope_buffer: Vec::new(),
      read_count: 0,
      write_count: 0,
      sample_rate: 1,
      cursor: true,
      hold: false,
      style: Style {
        foreground_colour: gdk::RGBA::black(),
        background_colour: gdk::RGBA::white(),
        line_size: 1.,
        antialias: false,
        reflect: false,
        cursor: None,
        width: 0,
        height: 0,
        gain: 1.0,
      },
    }
  }
}

pub trait Renderer {
  fn draw_scope<'a, I>(&self, &Style, I)
    where I: Iterator<Item=&'a f32>;
}
