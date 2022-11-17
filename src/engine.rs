use crate::browser::{self, LoopClosure};
use crate::sound;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::channel::mpsc::unbounded;
use futures::channel::{mpsc::UnboundedReceiver, oneshot::channel};
use js_sys::ArrayBuffer;
use serde::Deserialize;
use std::collections::HashMap;
use std::{cell::RefCell, rc::Rc, sync::Mutex};
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use web_sys::{AudioBuffer, AudioContext, HtmlElement};
use web_sys::{CanvasRenderingContext2d, HtmlImageElement};

#[async_trait(?Send)]
pub trait Game {
  fn draw(
    &self,
    renderer: &Renderer,
  );

  async fn initialize(&self) -> Result<Box<dyn Game>>;

  fn update(
    &mut self,
    keystate: &KeyState,
  );
}

const FRAME_SIZE: f32 = 1.0 / 60.0 * 1000.0;

pub struct GameLoop {
  accumulated_delta: f32,
  last_frame: f64,
}

type SharedLoopClosure = Rc<RefCell<Option<LoopClosure>>>;

impl GameLoop {
  pub async fn start(game: impl Game + 'static) -> Result<()> {
    let mut keyevent_receiver = prepare_input()?;
    let mut game = game.initialize().await?;
    let mut game_loop = GameLoop {
      accumulated_delta: 0.0,
      last_frame: browser::now()?,
    };
    let renderer = Renderer {
      context: browser::context()?,
    };
    let f: SharedLoopClosure = Rc::new(RefCell::new(None));
    let g = f.clone();
    let mut keystate = KeyState::new();
    *g.borrow_mut() = Some(browser::create_raf_closure(move |perf: f64| {
      process_input(&mut keystate, &mut keyevent_receiver);
      game_loop.accumulated_delta += (perf - game_loop.last_frame) as f32;
      while game_loop.accumulated_delta > FRAME_SIZE {
        game.update(&keystate);
        game_loop.accumulated_delta -= FRAME_SIZE;
      }
      game_loop.last_frame = perf;
      game.draw(&renderer);
      let _result: Result<i32, anyhow::Error> =
        browser::request_animation_frame(f.borrow().as_ref().unwrap());
    }));
    browser::request_animation_frame(
      g.borrow().as_ref().ok_or_else(|| anyhow!("GameLoop: Loop is None"))?,
    )?;
    Ok(())
  }
}

pub fn add_click_handler(elem: HtmlElement) -> UnboundedReceiver<()> {
  let (mut click_sender, click_receiver) = unbounded();
  let on_click = browser::closure_wrap(Box::new(move || {
    click_sender.start_send(());
  }) as Box<dyn FnMut()>);
  elem.set_onclick(Some(on_click.as_ref().unchecked_ref()));
  on_click.forget();
  click_receiver
}

pub async fn load_image(source: &str) -> Result<HtmlImageElement> {
  let image: HtmlImageElement = browser::new_image()?;
  let (complete_tx, complete_rx) = channel::<Result<()>>();
  let success_tx = Rc::new(Mutex::new(Some(complete_tx)));
  let error_tx = Rc::clone(&success_tx);
  let success_callback = browser::closure_once(move || {
    if let Some(success_tx) =
      success_tx.lock().ok().and_then(|mut opt| opt.take())
    {
      let _result = success_tx.send(Ok(()));
    }
  });
  let error_callback: Closure<dyn FnMut(JsValue)> =
    browser::closure_once(move |err| {
      if let Some(error_tx) =
        error_tx.lock().ok().and_then(|mut opt| opt.take())
      {
        let _result =
          error_tx.send(Err(anyhow!("Error Loading Image: {:#?}", err)));
      }
    });
  image.set_onload(Some(success_callback.as_ref().unchecked_ref()));
  image.set_onerror(Some(error_callback.as_ref().unchecked_ref()));
  image.set_src(source);
  complete_rx.await??;
  Ok(image)
}

enum KeyPress {
  KeyDown(web_sys::KeyboardEvent),
  KeyUp(web_sys::KeyboardEvent),
}

fn prepare_input() -> Result<UnboundedReceiver<KeyPress>> {
  let (keydown_sender, keyevent_receiver) = unbounded();
  let keydown_sender = Rc::new(RefCell::new(keydown_sender));
  let keyup_sender = Rc::clone(&keydown_sender);
  let onkeydown =
    browser::closure_wrap(Box::new(move |keycode: web_sys::KeyboardEvent| {
      let _result =
        keydown_sender.borrow_mut().start_send(KeyPress::KeyDown(keycode));
    }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);
  let onkeyup =
    browser::closure_wrap(Box::new(move |keycode: web_sys::KeyboardEvent| {
      let _result =
        keyup_sender.borrow_mut().start_send(KeyPress::KeyUp(keycode));
    }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);
  browser::window()?.set_onkeydown(Some(onkeydown.as_ref().unchecked_ref()));
  browser::window()?.set_onkeyup(Some(onkeyup.as_ref().unchecked_ref()));
  onkeydown.forget();
  onkeyup.forget();
  Ok(keyevent_receiver)
}

pub struct KeyState {
  pressed_keys: HashMap<String, web_sys::KeyboardEvent>,
}

impl KeyState {
  fn new() -> Self {
    KeyState {
      pressed_keys: HashMap::new(),
    }
  }

  pub fn is_pressed(
    &self,
    code: &str,
  ) -> bool {
    self.pressed_keys.contains_key(code)
  }

  fn set_pressed(
    &mut self,
    code: &str,
    event: web_sys::KeyboardEvent,
  ) {
    log!("set_pressed {}", code);
    self.pressed_keys.insert(code.into(), event);
  }

  fn set_released(
    &mut self,
    code: &str,
  ) {
    self.pressed_keys.remove(code);
  }
}

fn process_input(
  state: &mut KeyState,
  keyevent_receiver: &mut UnboundedReceiver<KeyPress>,
) {
  log!("process_input");
  loop {
    match keyevent_receiver.try_next() {
      Ok(None) => break,
      Err(_err) => break,
      Ok(Some(evt)) => match evt {
        KeyPress::KeyUp(evt) => state.set_released(&evt.code()),
        KeyPress::KeyDown(evt) => state.set_pressed(&evt.code(), evt),
      },
    }
  }
}

#[derive(Clone, Copy, Default)]
pub struct Point {
  pub x: i16,
  pub y: i16,
}

#[derive(Default)]
pub struct Rect {
  pub position: Point,
  pub width: i16,
  pub height: i16,
}

impl Rect {
  pub fn new(
    position: Point,
    width: i16,
    height: i16,
  ) -> Self {
    Rect {
      position,
      width,
      height,
    }
  }

  pub fn bottom(&self) -> i16 {
    self.position.y + self.height
  }

  pub fn intersects(
    &self,
    rect: &Rect,
  ) -> bool {
    self.x() < rect.right()
      && self.right() > rect.x()
      && self.y() < rect.bottom()
      && self.bottom() > rect.y()
  }

  pub fn new_from_x_y(
    x: i16,
    y: i16,
    width: i16,
    height: i16,
  ) -> Self {
    Rect::new(
      Point {
        x,
        y,
      },
      width,
      height,
    )
  }

  pub fn right(&self) -> i16 {
    self.position.x + self.width
  }

  pub fn set_x(
    &mut self,
    x: i16,
  ) {
    self.position.x = x
  }

  pub fn x(&self) -> i16 {
    self.position.x
  }

  pub fn y(&self) -> i16 {
    self.position.y
  }
}

pub struct Renderer {
  context: CanvasRenderingContext2d,
}

impl Renderer {
  pub fn clear(
    &self,
    rect: &Rect,
  ) {
    self.context.clear_rect(
      rect.position.x.into(),
      rect.position.y.into(),
      rect.width.into(),
      rect.height.into(),
    );
  }

  pub fn draw_entire_image(
    &self,
    image: &HtmlImageElement,
    position: &Point,
  ) {
    let x: f64 = position.x.into();
    let y: f64 = position.y.into();
    self
      .context
      .draw_image_with_html_image_element(image, x, y)
      .expect("Drawing is throwing exceptions! Unrecoverable error.");
  }

  pub fn draw_image(
    &self,
    image: &HtmlImageElement,
    frame: &Rect,
    destination: &Rect,
  ) {
    self.context.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
      image,
      frame.position.x.into(),
      frame.position.y.into(),
      frame.width.into(),
      frame.height.into(),
      destination.position.x.into(),
      destination.position.y.into(),
      destination.width.into(),
      destination.height.into(),
    ).expect("Drawin is throwing exceptions! Unrecoverable error.");
  }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cell {
  pub frame: SheetRect,
  pub sprite_source_size: SheetRect,
}

#[derive(Clone, Deserialize)]
pub struct Sheet {
  pub frames: HashMap<String, Cell>,
}

#[derive(Clone, Deserialize)]
pub struct SheetRect {
  pub x: i16,
  pub y: i16,
  pub w: i16,
  pub h: i16,
}

pub struct Image {
  bounding_box: Rect,
  element: HtmlImageElement,
}

impl Image {
  pub fn new(
    element: HtmlImageElement,
    position: Point,
  ) -> Self {
    let bounding_box =
      Rect::new(position, element.width() as i16, element.height() as i16);
    Self {
      bounding_box,
      element,
    }
  }

  pub fn bounding_box(&self) -> &Rect {
    &self.bounding_box
  }

  pub fn draw(
    &self,
    renderer: &Renderer,
  ) {
    renderer.draw_entire_image(&self.element, &self.bounding_box.position)
  }

  pub fn move_horizontally(
    &mut self,
    distance: i16,
  ) {
    self.set_x(self.bounding_box.x() + distance);
  }

  pub fn right(&self) -> i16 {
    self.bounding_box.right()
  }

  pub fn set_x(
    &mut self,
    x: i16,
  ) {
    self.bounding_box.set_x(x);
    self.bounding_box.set_x(x);
  }
}

pub struct SpriteSheet {
  image: HtmlImageElement,
  sheet: Sheet,
}

impl SpriteSheet {
  pub fn new(
    image: HtmlImageElement,
    sheet: Sheet,
  ) -> Self {
    SpriteSheet {
      image,
      sheet,
    }
  }

  pub fn cell(
    &self,
    name: &str,
  ) -> Option<&Cell> {
    self.sheet.frames.get(name)
  }

  pub fn draw(
    &self,
    renderer: &Renderer,
    source: &Rect,
    destination: &Rect,
  ) {
    renderer.draw_image(&self.image, source, destination);
  }
}

#[derive(Clone)]
pub struct Audio {
  context: AudioContext,
}

impl Audio {
  pub fn new() -> Result<Self> {
    Ok(Audio {
      context: sound::create_audio_context()?,
    })
  }

  pub async fn load_sound(
    &self,
    filename: &str,
  ) -> Result<Sound> {
    let array_buffer: ArrayBuffer =
      browser::fetch_array_buffer(filename).await?;
    let audio_buffer: AudioBuffer =
      sound::decode_audio_data(&self.context, &array_buffer).await?;
    Ok(Sound {
      buffer: audio_buffer,
    })
  }

  pub fn play_looping_sound(
    &self,
    sound: &Sound,
  ) -> Result<()> {
    sound::play_sound(&self.context, &sound.buffer, sound::Looping::YES)
  }

  pub fn play_sound(
    &self,
    sound: &Sound,
  ) -> Result<()> {
    sound::play_sound(&self.context, &sound.buffer, sound::Looping::NO)
  }
}

#[derive(Clone)]
pub struct Sound {
  buffer: AudioBuffer,
}
