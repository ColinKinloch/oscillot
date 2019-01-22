extern crate cairo;

impl super::Renderer for cairo::Context {
  fn draw_scope<'a, I>(&self, style: &super::Style, mut samples: I) 
    where I: Iterator<Item=&'a f32> {
    if style.reflect {
      self.scale(1f64, 1f64);
      self.translate(0., 0.);
    } else {
      self.scale(-1f64, 1f64);
      self.translate(-(style.width as f64), 0.);
    }

    let antialias_type = if style.antialias {
        cairo::Antialias::None
    } else {
        cairo::Antialias::Fast
    };

    self.set_antialias(antialias_type);

    {
      let ref c = style.foreground_colour;
      self.set_source_rgba(c.red, c.green, c.blue, c.alpha);
      self.set_line_width(style.line_size);
    }
    
    self.move_to(0., (1. - *samples.next().unwrap() as f64 * style.gain) * style.height as f64 / 2.);
    for (i, s) in samples.enumerate() {
      self.line_to(i as f64, (1. - *s as f64 * style.gain) * style.height as f64 / 2.);
    }
    
    if let Some(cursor) = style.cursor {
      let rec = cursor as f64;
      self.move_to(rec  as f64, 0.);
      self.line_to(rec, style.height as f64);
    }
    
    self.stroke();
  }
}
