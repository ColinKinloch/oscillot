extern crate jack;

extern crate gtk;
extern crate gdk;
extern crate glib;
extern crate gio;
extern crate cairo;

mod scope;

use gtk::prelude::*;
use gio::prelude::*;
use jack::prelude::*;

use std::sync::mpsc::{channel, Receiver};
use std::rc::Rc;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};

static APP_ID: &str = "org.colinkinloch.oscillot";
static APP_PATH: &str = "/org/colinkinloch/oscillot";

const RESOURCE_BYTES: &[u8] = include_bytes!("resources/oscillot.gresource");

thread_local!(
  static GLOBAL: RefCell<(Option<gtk::DrawingArea>, Option<Receiver<(usize, usize)>>)> = RefCell::new((None, None))
);

fn init_gui() {
  if gtk::init().is_err() {
    println!("Failed to initialize GTK.");
  }
  
  let resource = gio::Resource::new_from_data(&glib::Bytes::from_static(RESOURCE_BYTES)).unwrap();
  gio::resources_register(&resource);
}

fn main() {
  init_gui();

  let application = gtk::Application::new(Some(APP_ID), gio::ApplicationFlags::empty())
    .expect("Cannot create application.");
  application.set_resource_base_path(Some(APP_PATH));

  let quit_action = gio::SimpleAction::new("quit", None);
  application.add_action(&quit_action);
  {
    let application = application.clone();
    quit_action.connect_activate(move |_, _| application.quit());
  }

  let scope = Arc::new(Mutex::new(scope::Scope::new()));

  let (client, _status) = Client::new("oscillot", client_options::NO_START_SERVER).unwrap();
  let in_port = client.register_port("in", AudioInSpec::default()).unwrap();

  let (tx, rx) = channel();

  GLOBAL.with(move |global| {
    (*global.borrow_mut()).1 = Some(rx)
  });

  let process = {
    let scope = scope.clone();
    ClosureProcessHandler::new(move |_: &Client, ps: &ProcessScope| -> JackControl {
      let in_p = AudioInPort::new(&in_port, ps);
      {
        let in_p_slice: &[f32] = &in_p;
        let mut scope = scope.lock().unwrap();
        let mut wc = scope.write_count;
        let start_wc = wc;

        if wc > scope.samples.len() {
          wc = 0;
        }

        for s in in_p_slice.iter() {
          scope.samples[wc] = *s;
          //scope.scope_buffer.push_front(*s);
          wc += 1;
          if wc > scope.samples.len() - 1 {
            wc = 0;
          }
        }
        scope.write_count = wc;
        
        if !scope.hold && scope.cursor {
          scope.style.cursor = Some(wc / scope.sample_rate);
        } else {
          scope.style.cursor = None;
        }

        tx.send((start_wc, wc)).expect("tx fail");
        glib::idle_add(trigger_render);
      }
      JackControl::Continue
    })
  };
  let _active_client = AsyncClient::new(client, (), process).unwrap();

  let running = Rc::new(RefCell::new(true));

  {
    let running = running.clone();
    application.connect_activate(move |application| activate(application, scope.clone(), running.clone()));
  }
  {
    let running = running.clone();
    application.connect_shutdown(move |application| shutdown(application, running.clone()));
  }

  application.run(&std::env::args().collect::<Vec<_>>());
}

fn trigger_render() -> glib::Continue {
  GLOBAL.with(|global| {
    if let (Some(ref graph_area), Some(ref rx)) = *global.borrow() {
      if let Ok((start_wc, end_wc)) = rx.try_recv() {
        // TODO: Calculate refresh range
        // TODO: Handle non cursored
        if end_wc < start_wc {
          graph_area.queue_draw_area(
            start_wc as i32, 0,
            graph_area.get_allocated_width(), graph_area.get_allocated_height());
        }
        graph_area.queue_draw_area(
          0, 0,
          end_wc as i32, graph_area.get_allocated_height());
      }
    }
  });
  glib::Continue(false)
}

fn activate(application: &gtk::Application, scope: Arc<Mutex<scope::Scope>>, running: Rc<RefCell<bool>>) {
  let builder = gtk::Builder::new_from_resource("/org/colinkinloch/oscillot/ui/oscillot.ui");

  let window = builder.get_object::<gtk::ApplicationWindow>("scope-window")
    .expect("Cannot get main window.");
  window.set_application(Some(application));
  window.set_default_size(1280, 720);
  //window.show_all();

  let css = gtk::CssProvider::new();
  window.get_style_context().add_provider(&css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);

  let style_popover_grid = builder.get_object::<gtk::Grid>("style-popover-grid")
    .expect("Cannot get style popover grid");

  let settings_popover_grid = builder.get_object::<gtk::Grid>("settings-popover-grid")
    .expect("Cannot get settings popover grid");

  let foreground_colour_picker = builder.get_object::<gtk::ColorButton>("foreground-colour-picker")
    .expect("Cannot get bg cp");
  let background_colour_picker = builder.get_object::<gtk::ColorButton>("background-colour-picker")
    .expect("Cannot get bg cp");

  let graph_area = builder.get_object::<gtk::DrawingArea>("graph-area")
    .expect("Cannot get graph area.");

  {
    let graph_area = graph_area.clone();
    GLOBAL.with(move |global| {
      (*global.borrow_mut()).0 = Some(graph_area.clone())
    });
  }

  let line_size_adjustment = builder.get_object::<gtk::Adjustment>("line-size")
    .expect("Cannot get line-size adjustment");

  let antialias_switch = builder.get_object::<gtk::Switch>("antialias-switch")
    .expect("Cannot get antialias toggle button");

  let cursor_switch = builder.get_object::<gtk::Switch>("cursor-switch")
    .expect("Cannot get cursor toggle button");

  let sample_rate_adjustment = builder.get_object::<gtk::Adjustment>("sample-rate")
    .expect("Cannot get sample rate adjustment");

  let gain_adjustment = builder.get_object::<gtk::Adjustment>("gain")
    .expect("Cannot get gain adjustment");
  let gain_scale = builder.get_object::<gtk::Scale>("gain-scale")
    .expect("Cannot get gain scale");
  {
    let pos = gtk::PositionType::Bottom;
    gain_scale.add_mark(0.5, pos, None);
    gain_scale.add_mark(1.0, pos, None);
    gain_scale.add_mark(2.0, pos, None);
  }

  use gio::ActionMapExt;
  let reverse_action = gio::SimpleAction::new_stateful("reverse",
    Some(glib::VariantTy::new("b").unwrap()), &glib::Variant::from(false));
  window.add_action(&reverse_action);

  let hold_action = gio::SimpleAction::new_stateful("hold",
    Some(glib::VariantTy::new("b").unwrap()), &glib::Variant::from(false));
  window.add_action(&hold_action);

  {
    let scope = scope.clone();
    use gio::SimpleActionExt;
    reverse_action.connect_change_state(move |action, _state| {
      let mut scope = scope.lock().unwrap();
      scope.style.reflect = !scope.style.reflect;
      action.set_state(&glib::Variant::from(scope.style.reflect));
    });
  }

  {
    let scope = scope.clone();
    use gio::SimpleActionExt;
    let hold_toggle = builder.get_object::<gtk::ToggleButton>("hold-toggle")
      .expect("could not get hold toggle");
    hold_action.connect_change_state(move |action, _state| {
      let state = hold_toggle.get_active();
      action.set_state(&glib::Variant::from(state));
      scope.lock().unwrap().hold = state;
    });
  }

  {
    background_colour_picker.connect_color_set(move |widget| {
      let c = widget.get_rgba();
      let css_string = format!(
        ".background {{background-color: rgba({}, {}, {}, {});}}",
        c.red * 255., c.green * 255., c.blue * 255., c.alpha
      );
      css.load_from_data(css_string.as_bytes())
        .expect("failed to load background colour css");
    });
  }

  {
    let scope = scope.clone();
    foreground_colour_picker.connect_color_set(move |widget| {
      scope.lock().unwrap().style.foreground_colour = widget.get_rgba();
    });
  }

  {
    let scope = scope.clone();
    line_size_adjustment.connect_value_changed(move |widget| {
      scope.lock().unwrap().style.line_size = widget.get_value();
    });
  }

  {
    let scope = scope.clone();
    antialias_switch.connect_state_set(move |_, state| {
      scope.lock().unwrap().style.antialias = state;
      Inhibit(false)
    });
  }

  {
    let scope = scope.clone();
    cursor_switch.connect_state_set(move |_, state| {
      scope.lock().unwrap().cursor = state;
      Inhibit(false)
    });
  }

  {
    let scope = scope.clone();
    sample_rate_adjustment.connect_value_changed(move |widget| {
      scope.lock().unwrap().sample_rate = widget.get_value() as usize;
    });
  }

  {
    let scope = scope.clone();
    gain_adjustment.connect_value_changed(move |widget| {
      scope.lock().unwrap().style.gain = widget.get_value() as f64;
    });
  }

  {
    let scope = scope.clone();
    graph_area.connect_draw(move |graph_area, cr| {
      let mut scope = scope.lock().unwrap();
      let width = graph_area.get_allocated_width() as u32;
      let height = graph_area.get_allocated_height() as u32;
      let sample_rate = scope.sample_rate;
      scope.style.width = width;
      scope.style.height = height;
      //scope.draw(&cr);
      scope.scope_buffer.resize(width as usize * sample_rate as usize, 0.);
      scope.samples.resize(width as usize * sample_rate as usize, 0.);
      {
        let mut rc = scope.write_count as isize - 1;
        if rc < 0 {
          rc = scope.samples.len() as isize - 1;
        }
        
        let mut iterator = Box::new(scope.samples.iter().cycle()) as Box<Iterator<Item=&f32>>;
        if scope.hold {
          iterator = Box::new(iterator.skip(rc as usize)) as Box<Iterator<Item=&f32>>;
        }
        iterator = Box::new(iterator.step_by(sample_rate as usize)
          .take(width as usize))
            as Box<Iterator<Item=&f32>>;
        
        use scope::Renderer;
        cr.draw_scope(&scope.style, iterator);
      }
      Inhibit(false)
    });
  }

  {
    /*idle_add(move || {
      //graph_area.queue_draw();
      let scope = scope.lock().unwrap();
      if let Ok((start_wc, end_wc)) = scope.rx.try_recv() {
          let diff_wc = (end_wc as i32 - start_wc as i32) / 10;
          graph_area.queue_draw_area(start_wc as i32, 0, start_wc as i32 + diff_wc, graph_area.get_allocated_height());
      }
      Continue(*running.borrow())
  });*/
  }

  style_popover_grid.show_all();
  settings_popover_grid.show_all();
  window.show_all();
}

fn shutdown(_application: &gtk::Application, running: Rc<RefCell<bool>>) {
  //active_client.deactivate().unwrap();
  *running.borrow_mut() = false;
  println!("Shutting down!");
}
