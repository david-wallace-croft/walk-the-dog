use std::rc::Rc;

use self::red_hat_boy_states::*;
use crate::browser::{self};
use crate::engine::{
  self, Audio, Cell, Game, Image, KeyState, Point, Rect, Renderer, Sheet,
  Sound, SpriteSheet,
};
use crate::segments::platform_and_stone;
use crate::segments::stone_and_platform;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::channel::mpsc::UnboundedReceiver;
use rand::prelude::*;
use rand::thread_rng;
use wasm_bindgen::JsValue;
use web_sys::HtmlImageElement;

const HEIGHT: i16 = 600;
const OBSTACLE_BUFFER: i16 = 20;
const TIMELINE_MINIMUM: i16 = 1000;

#[derive(Clone)]
enum RedHatBoyStateMachine {
  Falling(RedHatBoyState<Falling>),
  Idle(RedHatBoyState<Idle>),
  Jumping(RedHatBoyState<Jumping>),
  KnockedOut(RedHatBoyState<KnockedOut>),
  Running(RedHatBoyState<Running>),
  Sliding(RedHatBoyState<Sliding>),
}

pub enum Event {
  Jump,
  KnockOut,
  Land(i16),
  Run,
  Slide,
  Update,
}

pub trait Obstacle {
  fn check_intersection(
    &self,
    boy: &mut RedHatBoy,
  );

  fn draw(
    &self,
    renderer: &Renderer,
  );

  fn move_horizontally(
    &mut self,
    x: i16,
  );

  fn right(&self) -> i16;
}

pub struct Barrier {
  image: Image,
}

impl Barrier {
  pub fn new(image: Image) -> Self {
    Barrier {
      image,
    }
  }
}

impl Obstacle for Barrier {
  fn check_intersection(
    &self,
    boy: &mut RedHatBoy,
  ) {
    if boy.bounding_box().intersects(self.image.bounding_box()) {
      boy.knock_out()
    }
  }

  fn draw(
    &self,
    renderer: &Renderer,
  ) {
    self.image.draw(renderer);
  }

  fn move_horizontally(
    &mut self,
    x: i16,
  ) {
    self.image.move_horizontally(x);
  }

  fn right(&self) -> i16 {
    self.image.right()
  }
}

pub struct Platform {
  bounding_boxes: Vec<Rect>,
  position: Point,
  sheet: Rc<SpriteSheet>,
  sprites: Vec<Cell>,
}

impl Platform {
  pub fn new(
    bounding_boxes: &[Rect],
    position: Point,
    sheet: Rc<SpriteSheet>,
    sprite_names: &[&str],
  ) -> Self {
    let sprites = sprite_names
      .iter()
      .filter_map(|sprite_name| sheet.cell(sprite_name).cloned())
      .collect();
    let bounding_boxes = bounding_boxes
      .iter()
      .map(|bounding_box| {
        Rect::new_from_x_y(
          bounding_box.x() + position.x,
          bounding_box.y() + position.y,
          bounding_box.width,
          bounding_box.height,
        )
      })
      .collect();
    Platform {
      bounding_boxes,
      position,
      sheet,
      sprites,
    }
  }

  fn bounding_boxes(&self) -> &Vec<Rect> {
    &self.bounding_boxes
  }
}

impl Obstacle for Platform {
  fn check_intersection(
    &self,
    boy: &mut RedHatBoy,
  ) {
    if let Some(box_to_land_on) = self
      .bounding_boxes()
      .iter()
      .find(|&bounding_box| boy.bounding_box().intersects(bounding_box))
    {
      if boy.velocity_y() > 0 && boy.pos_y() < self.position.y {
        boy.land_on(box_to_land_on.y());
      } else {
        boy.knock_out();
      }
    }
  }

  fn draw(
    &self,
    renderer: &Renderer,
  ) {
    let mut x = 0;
    self.sprites.iter().for_each(|sprite| {
      self.sheet.draw(
        renderer,
        &Rect::new_from_x_y(
          sprite.frame.x,
          sprite.frame.y,
          sprite.frame.w,
          sprite.frame.h,
        ),
        &Rect::new_from_x_y(
          self.position.x + x,
          self.position.y,
          sprite.frame.w,
          sprite.frame.h,
        ),
      );
      x += sprite.frame.w;
    });
  }

  fn move_horizontally(
    &mut self,
    x: i16,
  ) {
    self.position.x += x;
    self.bounding_boxes.iter_mut().for_each(|bounding_box| {
      bounding_box.set_x(bounding_box.position.x + x);
    });
  }

  fn right(&self) -> i16 {
    self.bounding_boxes().last().unwrap_or(&Rect::default()).right()
  }
}

impl RedHatBoyStateMachine {
  fn context(&self) -> &RedHatBoyContext {
    match self {
      RedHatBoyStateMachine::Falling(state) => state.context(),
      RedHatBoyStateMachine::Idle(state) => state.context(),
      RedHatBoyStateMachine::Jumping(state) => state.context(),
      RedHatBoyStateMachine::KnockedOut(state) => state.context(),
      RedHatBoyStateMachine::Running(state) => state.context(),
      RedHatBoyStateMachine::Sliding(state) => state.context(),
    }
  }

  fn frame_name(&self) -> &str {
    match self {
      RedHatBoyStateMachine::Falling(state) => state.frame_name(),
      RedHatBoyStateMachine::Idle(state) => state.frame_name(),
      RedHatBoyStateMachine::Jumping(state) => state.frame_name(),
      RedHatBoyStateMachine::KnockedOut(state) => state.frame_name(),
      RedHatBoyStateMachine::Running(state) => state.frame_name(),
      RedHatBoyStateMachine::Sliding(state) => state.frame_name(),
    }
  }

  fn knocked_out(&self) -> bool {
    matches!(self, RedHatBoyStateMachine::KnockedOut(_))
  }

  fn transition(
    self,
    event: Event,
  ) -> Self {
    match (self.clone(), event) {
      (RedHatBoyStateMachine::Idle(state), Event::Run) => state.run().into(),
      (RedHatBoyStateMachine::Idle(state), Event::Update) => {
        state.update().into()
      },
      (RedHatBoyStateMachine::Falling(state), Event::Update) => {
        state.update().into()
      },
      (RedHatBoyStateMachine::Jumping(state), Event::KnockOut) => {
        state.knock_out().into()
      },
      (RedHatBoyStateMachine::Jumping(state), Event::Land(position)) => {
        state.land_on(position).into()
      },
      (RedHatBoyStateMachine::Jumping(state), Event::Update) => {
        state.update().into()
      },
      (RedHatBoyStateMachine::KnockedOut(state), Event::Update) => {
        state.update().into()
      },
      (RedHatBoyStateMachine::Running(state), Event::Jump) => {
        state.jump().into()
      },
      (RedHatBoyStateMachine::Running(state), Event::KnockOut) => {
        state.knock_out().into()
      },
      (RedHatBoyStateMachine::Running(state), Event::Land(position)) => {
        state.land_on(position).into()
      },
      (RedHatBoyStateMachine::Running(state), Event::Slide) => {
        state.slide().into()
      },
      (RedHatBoyStateMachine::Running(state), Event::Update) => {
        state.update().into()
      },
      (RedHatBoyStateMachine::Sliding(state), Event::KnockOut) => {
        state.knock_out().into()
      },
      (RedHatBoyStateMachine::Sliding(state), Event::Land(position)) => {
        state.land_on(position).into()
      },
      (RedHatBoyStateMachine::Sliding(state), Event::Update) => {
        state.update().into()
      },
      _ => self,
    }
  }

  fn update(self) -> Self {
    self.transition(Event::Update)
  }
}

impl From<FallingEndState> for RedHatBoyStateMachine {
  fn from(end_state: FallingEndState) -> Self {
    match end_state {
      FallingEndState::Complete(knocked_out_state) => knocked_out_state.into(),
      FallingEndState::Falling(falling_state) => falling_state.into(),
    }
  }
}

impl From<JumpingEndState> for RedHatBoyStateMachine {
  fn from(end_state: JumpingEndState) -> Self {
    match end_state {
      JumpingEndState::Jumping(jumping_state) => jumping_state.into(),
      JumpingEndState::Landing(running_state) => running_state.into(),
    }
  }
}

impl From<RedHatBoyState<Falling>> for RedHatBoyStateMachine {
  fn from(state: RedHatBoyState<Falling>) -> Self {
    RedHatBoyStateMachine::Falling(state)
  }
}

impl From<RedHatBoyState<Idle>> for RedHatBoyStateMachine {
  fn from(state: RedHatBoyState<Idle>) -> Self {
    RedHatBoyStateMachine::Idle(state)
  }
}

impl From<RedHatBoyState<Jumping>> for RedHatBoyStateMachine {
  fn from(state: RedHatBoyState<Jumping>) -> Self {
    RedHatBoyStateMachine::Jumping(state)
  }
}

impl From<RedHatBoyState<KnockedOut>> for RedHatBoyStateMachine {
  fn from(state: RedHatBoyState<KnockedOut>) -> Self {
    RedHatBoyStateMachine::KnockedOut(state)
  }
}

impl From<RedHatBoyState<Running>> for RedHatBoyStateMachine {
  fn from(state: RedHatBoyState<Running>) -> Self {
    RedHatBoyStateMachine::Running(state)
  }
}

impl From<RedHatBoyState<Sliding>> for RedHatBoyStateMachine {
  fn from(state: RedHatBoyState<Sliding>) -> Self {
    RedHatBoyStateMachine::Sliding(state)
  }
}

impl From<SlidingEndState> for RedHatBoyStateMachine {
  fn from(end_state: SlidingEndState) -> Self {
    match end_state {
      SlidingEndState::Complete(running_state) => running_state.into(),
      SlidingEndState::Sliding(sliding_state) => sliding_state.into(),
    }
  }
}

pub struct RedHatBoy {
  state_machine: RedHatBoyStateMachine,
  sprite_sheet: Sheet,
  image: HtmlImageElement,
}

impl RedHatBoy {
  fn new(
    audio: Audio,
    image: HtmlImageElement,
    jump_sound: Sound,
    sheet: Sheet,
  ) -> Self {
    RedHatBoy {
      state_machine: RedHatBoyStateMachine::Idle(RedHatBoyState::new(
        audio, jump_sound,
      )),
      sprite_sheet: sheet,
      image,
    }
  }

  fn bounding_box(&self) -> Rect {
    const X_OFFSET: i16 = 18;
    const Y_OFFSET: i16 = 14;
    const WIDTH_OFFSET: i16 = 28;
    let mut bounding_box = self.destination_box();
    bounding_box.position.x += X_OFFSET;
    bounding_box.width -= WIDTH_OFFSET;
    bounding_box.position.y += Y_OFFSET;
    bounding_box.height -= Y_OFFSET;
    bounding_box
  }

  fn current_sprite(&self) -> Option<&Cell> {
    self.sprite_sheet.frames.get(&self.frame_name())
  }

  fn destination_box(&self) -> Rect {
    let sprite = self.current_sprite().expect("Cell not found");
    Rect {
      position: Point {
        x: self.state_machine.context().position.x
          + sprite.sprite_source_size.x,
        y: self.state_machine.context().position.y
          + sprite.sprite_source_size.y,
      },
      width: sprite.frame.w,
      height: sprite.frame.h,
    }
  }

  fn draw(
    &self,
    renderer: &Renderer,
  ) {
    let sprite = self.current_sprite().expect("Cell not found");
    renderer.draw_image(
      &self.image,
      &Rect {
        position: Point {
          x: sprite.frame.x,
          y: sprite.frame.y,
        },
        width: sprite.frame.w,
        height: sprite.frame.h,
      },
      &self.destination_box(),
    );
  }

  fn frame_name(&self) -> String {
    format!(
      "{} ({}).png",
      self.state_machine.frame_name(),
      (self.state_machine.context().frame / 3) + 1
    )
  }

  fn jump(&mut self) {
    log!("jump!");
    self.state_machine = self.state_machine.clone().transition(Event::Jump);
  }

  fn knock_out(&mut self) {
    self.state_machine = self.state_machine.clone().transition(Event::KnockOut);
  }

  fn knocked_out(&self) -> bool {
    self.state_machine.knocked_out()
  }

  pub fn land_on(
    &mut self,
    position: i16,
  ) {
    self.state_machine =
      self.state_machine.clone().transition(Event::Land(position));
  }

  fn pos_y(&self) -> i16 {
    self.state_machine.context().position.y
  }

  fn reset(boy: Self) -> Self {
    RedHatBoy::new(
      boy.state_machine.context().audio.clone(),
      boy.image,
      boy.state_machine.context().jump_sound.clone(),
      boy.sprite_sheet,
    )
  }

  fn run_right(&mut self) {
    self.state_machine = self.state_machine.clone().transition(Event::Run);
  }

  fn slide(&mut self) {
    self.state_machine = self.state_machine.clone().transition(Event::Slide);
  }

  fn update(&mut self) {
    self.state_machine = self.state_machine.clone().update();
  }

  fn velocity_y(&self) -> i16 {
    self.state_machine.context().velocity.y
  }

  fn walking_speed(&self) -> i16 {
    self.state_machine.context().velocity.x
  }
}

struct Walk {
  backgrounds: [Image; 2],
  boy: RedHatBoy,
  obstacle_sheet: Rc<SpriteSheet>,
  obstacles: Vec<Box<dyn Obstacle>>,
  stone: HtmlImageElement,
  timeline: i16,
}

impl Walk {
  fn draw(
    &self,
    renderer: &Renderer,
  ) {
    self.backgrounds.iter().for_each(|background| {
      background.draw(renderer);
    });
    self.boy.draw(renderer);
    self.obstacles.iter().for_each(|obstacle| {
      obstacle.draw(renderer);
    });
  }

  fn knocked_out(&self) -> bool {
    self.boy.knocked_out()
  }

  fn generate_next_segment(&mut self) {
    let mut rng = thread_rng();
    let next_segment = rng.gen_range(0..2);
    let mut next_obstacles = match next_segment {
      0 => stone_and_platform(
        self.timeline + OBSTACLE_BUFFER,
        self.obstacle_sheet.clone(),
        self.stone.clone(),
      ),
      1 => platform_and_stone(
        self.timeline + OBSTACLE_BUFFER,
        self.obstacle_sheet.clone(),
        self.stone.clone(),
      ),
      _ => vec![],
    };
    self.timeline = rightmost(&next_obstacles);
    self.obstacles.append(&mut next_obstacles);
  }

  fn reset(walk: Self) -> Self {
    let starting_obstacles =
      stone_and_platform(0, walk.obstacle_sheet.clone(), walk.stone.clone());
    let timeline = rightmost(&starting_obstacles);
    Walk {
      backgrounds: walk.backgrounds,
      boy: RedHatBoy::reset(walk.boy),
      obstacle_sheet: walk.obstacle_sheet,
      obstacles: starting_obstacles,
      stone: walk.stone,
      timeline,
    }
  }

  fn velocity(&self) -> i16 {
    -self.boy.walking_speed()
  }
}

pub struct WalkTheDog {
  machine: Option<WalkTheDogStateMachine>,
}

struct Ready;
struct Walking;
struct GameOver {
  new_game_event: UnboundedReceiver<()>,
}

enum WalkTheDogStateMachine {
  Ready(WalkTheDogState<Ready>),
  Walking(WalkTheDogState<Walking>),
  GameOver(WalkTheDogState<GameOver>),
}

struct WalkTheDogState<T> {
  _state: T,
  walk: Walk,
}

impl<T> WalkTheDogState<T> {
  fn draw(
    &self,
    renderer: &Renderer,
  ) {
    self.walk.draw(renderer);
  }
}

impl WalkTheDogStateMachine {
  fn new(walk: Walk) -> Self {
    WalkTheDogStateMachine::Ready(WalkTheDogState::new(walk))
  }

  fn draw(
    &self,
    renderer: &Renderer,
  ) {
    match self {
      WalkTheDogStateMachine::GameOver(state) => state.draw(renderer),
      WalkTheDogStateMachine::Ready(state) => state.draw(renderer),
      WalkTheDogStateMachine::Walking(state) => state.draw(renderer),
    }
  }

  fn update(
    self,
    keystate: &KeyState,
  ) -> Self {
    match self {
      WalkTheDogStateMachine::GameOver(state) => state.update().into(),
      WalkTheDogStateMachine::Ready(state) => state.update(keystate).into(),
      WalkTheDogStateMachine::Walking(state) => state.update(keystate).into(),
    }
  }
}

impl WalkTheDog {
  pub fn new() -> Self {
    WalkTheDog {
      machine: None,
    }
  }
}

impl WalkTheDogState<GameOver> {
  fn new_game(self) -> WalkTheDogState<Ready> {
    let _result: Result<()> = browser::hide_ui();
    WalkTheDogState {
      _state: Ready,
      walk: Walk::reset(self.walk),
    }
  }

  fn update(mut self) -> GameOverEndState {
    if self._state.new_game_pressed() {
      GameOverEndState::Complete(self.new_game())
    } else {
      GameOverEndState::Continue(self)
    }
  }
}

enum GameOverEndState {
  Continue(WalkTheDogState<GameOver>),
  Complete(WalkTheDogState<Ready>),
}

impl From<GameOverEndState> for WalkTheDogStateMachine {
  fn from(state: GameOverEndState) -> Self {
    match state {
      GameOverEndState::Continue(game_over) => game_over.into(),
      GameOverEndState::Complete(ready) => ready.into(),
    }
  }
}

impl GameOver {
  fn new_game_pressed(&mut self) -> bool {
    matches!(self.new_game_event.try_next(), Ok(Some(())))
  }
}

enum ReadyEndState {
  Continue(WalkTheDogState<Ready>),
  Complete(WalkTheDogState<Walking>),
}

impl WalkTheDogState<Ready> {
  fn new(walk: Walk) -> WalkTheDogState<Ready> {
    WalkTheDogState {
      _state: Ready,
      walk,
    }
  }
  fn start_running(mut self) -> WalkTheDogState<Walking> {
    self.run_right();
    WalkTheDogState {
      _state: Walking,
      walk: self.walk,
    }
  }

  fn run_right(&mut self) {
    self.walk.boy.run_right();
  }

  fn update(
    mut self,
    keystate: &KeyState,
  ) -> ReadyEndState {
    self.walk.boy.update();
    if keystate.is_pressed("ArrowRight") {
      ReadyEndState::Complete(self.start_running())
    } else {
      ReadyEndState::Continue(self)
    }
  }
}

enum WalkingEndState {
  Continue(WalkTheDogState<Walking>),
  Complete(WalkTheDogState<GameOver>),
}

impl From<WalkingEndState> for WalkTheDogStateMachine {
  fn from(state: WalkingEndState) -> Self {
    match state {
      WalkingEndState::Continue(walking) => walking.into(),
      WalkingEndState::Complete(game_over) => game_over.into(),
    }
  }
}

impl WalkTheDogState<Walking> {
  fn end_game(self) -> WalkTheDogState<GameOver> {
    let receiver = browser::draw_ui("<button id='new_game'>New Game</button>")
      .and_then(|_unit| browser::find_html_element_by_id("new_game"))
      .map(engine::add_click_handler)
      .unwrap();
    WalkTheDogState {
      _state: GameOver {
        new_game_event: receiver,
      },
      walk: self.walk,
    }
  }

  fn update(
    mut self,
    keystate: &KeyState,
  ) -> WalkingEndState {
    if keystate.is_pressed("ArrowDown") {
      log!("ArrowDown");
      self.walk.boy.slide();
    }
    if keystate.is_pressed("Space") {
      log!("Space");
      self.walk.boy.jump();
    }
    self.walk.boy.update();
    let walking_speed = self.walk.velocity();
    let [first_background, second_background] = &mut self.walk.backgrounds;
    first_background.move_horizontally(walking_speed);
    second_background.move_horizontally(walking_speed);
    if first_background.right() < 0 {
      first_background.set_x(second_background.right());
    }
    if second_background.right() < 0 {
      second_background.set_x(first_background.right());
    }
    self.walk.obstacles.retain(|obstacle| obstacle.right() > 0);
    self.walk.obstacles.iter_mut().for_each(|obstacle| {
      obstacle.move_horizontally(walking_speed);
      obstacle.check_intersection(&mut self.walk.boy);
    });
    if self.walk.timeline < TIMELINE_MINIMUM {
      self.walk.generate_next_segment();
    } else {
      self.walk.timeline += walking_speed;
    }
    if self.walk.knocked_out() {
      WalkingEndState::Complete(self.end_game())
    } else {
      WalkingEndState::Continue(self)
    }
  }
}

impl From<ReadyEndState> for WalkTheDogStateMachine {
  fn from(state: ReadyEndState) -> Self {
    match state {
      ReadyEndState::Continue(ready) => ready.into(),
      ReadyEndState::Complete(walking) => walking.into(),
    }
  }
}

impl From<WalkTheDogState<GameOver>> for WalkTheDogStateMachine {
  fn from(state: WalkTheDogState<GameOver>) -> Self {
    WalkTheDogStateMachine::GameOver(state)
  }
}

impl From<WalkTheDogState<Ready>> for WalkTheDogStateMachine {
  fn from(state: WalkTheDogState<Ready>) -> Self {
    WalkTheDogStateMachine::Ready(state)
  }
}

impl From<WalkTheDogState<Walking>> for WalkTheDogStateMachine {
  fn from(state: WalkTheDogState<Walking>) -> Self {
    WalkTheDogStateMachine::Walking(state)
  }
}

#[async_trait(?Send)]
impl Game for WalkTheDog {
  fn draw(
    &self,
    renderer: &Renderer,
  ) {
    renderer.clear(&Rect {
      position: Point {
        x: 0,
        y: 0,
      },
      width: 600,
      height: 600,
    });
    if let Some(machine) = &self.machine {
      machine.draw(renderer);
    }
  }

  async fn initialize(&self) -> Result<Box<dyn Game>> {
    match self.machine {
      None => {
        let json: JsValue = browser::fetch_json("rhb.json").await?;
        let sheet: Sheet = serde_wasm_bindgen::from_value(json).unwrap();
        let background: HtmlImageElement = engine::load_image("BG.png").await?;
        let stone: HtmlImageElement = engine::load_image("Stone.png").await?;
        let tiles = browser::fetch_json("tiles.json").await?;
        let sprite_sheet = Rc::new(SpriteSheet::new(
          engine::load_image("tiles.png").await?,
          serde_wasm_bindgen::from_value(tiles).unwrap(),
        ));
        let image: HtmlImageElement = engine::load_image("rhb.png").await?;
        let audio = Audio::new()?;
        let sound = audio.load_sound("SFX_Jump_23.mp3").await?;
        let background_music = audio.load_sound("background_song.mp3").await?;
        audio.play_looping_sound(&background_music)?;
        let rhb: RedHatBoy = RedHatBoy::new(audio, image, sound, sheet);
        let background_width = background.width() as i16;
        let backgrounds = [
          Image::new(
            background.clone(),
            Point {
              x: 0,
              y: 0,
            },
          ),
          Image::new(
            background,
            Point {
              x: background_width,
              y: 0,
            },
          ),
        ];
        // let sprite_sheet_clone: Rc<SpriteSheet> = sprite_sheet.clone();
        let starting_obstacles =
          stone_and_platform(0, sprite_sheet.clone(), stone.clone());
        let timeline = rightmost(&starting_obstacles);
        let machine = WalkTheDogStateMachine::new(Walk {
          boy: rhb,
          backgrounds,
          obstacle_sheet: sprite_sheet,
          obstacles: starting_obstacles,
          stone,
          timeline,
        });
        Ok(Box::new(WalkTheDog {
          machine: Some(machine),
        }))
      },
      Some(_) => Err(anyhow!("Error: Game is already initialized!")),
    }
  }

  fn update(
    &mut self,
    keystate: &KeyState,
  ) {
    if let Some(machine) = self.machine.take() {
      self.machine.replace(machine.update(keystate));
    }
    assert!(self.machine.is_some());
  }
}

mod red_hat_boy_states {

  use super::HEIGHT;
  use crate::engine::{Audio, Point, Sound};

  const FALLING_FRAME_NAME: &str = "Dead";
  const FALLING_FRAMES: u8 = 29; // 10 'Dead' frames in the sheet, * 3 - 1
  const FLOOR: i16 = 479;
  const GRAVITY: i16 = 1;
  const IDLE_FRAME_NAME: &str = "Idle";
  const IDLE_FRAMES: u8 = 29;
  const JUMP_FRAME_NAME: &str = "Jump";
  const JUMP_SPEED: i16 = -25;
  const JUMPING_FRAMES: u8 = 35; // TODO: why is this 35?
  const PLAYER_HEIGHT: i16 = HEIGHT - FLOOR;
  const RUN_FRAME_NAME: &str = "Run";
  const RUNNING_FRAMES: u8 = 23;
  const RUNNING_SPEED: i16 = 4;
  const SLIDING_FRAMES: u8 = 14;
  const SLIDING_FRAME_NAME: &str = "Slide";
  const STARTING_POINT: i16 = -20;
  const TERMINAL_VELOCITY: i16 = 20;

  #[derive(Clone, Copy)]
  pub struct Falling;

  #[derive(Clone, Copy)]
  pub struct Idle;

  #[derive(Clone, Copy)]
  pub struct Jumping;

  #[derive(Clone, Copy)]
  pub struct KnockedOut;

  #[derive(Clone, Copy)]
  pub struct Running;

  #[derive(Clone, Copy)]
  pub struct Sliding;

  #[derive(Clone)]
  pub struct RedHatBoyState<S> {
    context: RedHatBoyContext,
    _state: S,
  }

  #[derive(Clone)]
  pub struct RedHatBoyContext {
    pub audio: Audio,
    pub frame: u8,
    pub jump_sound: Sound,
    pub position: Point,
    pub velocity: Point,
  }

  impl RedHatBoyContext {
    fn play_jump_sound(self) -> Self {
      if let Err(err) = self.audio.play_sound(&self.jump_sound) {
        log!("Error playing jump sound {:#?}", err);
      }
      self
    }

    fn reset_frame(mut self) -> Self {
      self.frame = 0;
      self
    }

    fn run_right(mut self) -> Self {
      self.velocity.x += RUNNING_SPEED;
      self
    }

    fn set_on(
      mut self,
      position: i16,
    ) -> Self {
      let position = position - PLAYER_HEIGHT;
      self.position.y = position;
      self
    }

    fn set_vertical_velocity(
      mut self,
      y: i16,
    ) -> Self {
      self.velocity.y = y;
      self
    }

    fn stop(mut self) -> Self {
      self.velocity.x = 0;
      self
    }

    pub fn update(
      mut self,
      frame_count: u8,
    ) -> Self {
      if self.velocity.y < TERMINAL_VELOCITY {
        self.velocity.y += GRAVITY;
      }
      if self.frame < frame_count {
        self.frame += 1;
      } else {
        self.frame = 0;
      }
      self.position.y += self.velocity.y;
      if self.position.y > FLOOR {
        self.position.y = FLOOR;
      }
      self
    }
  }

  impl<S> RedHatBoyState<S> {
    pub fn context(&self) -> &RedHatBoyContext {
      &self.context
    }
  }

  impl RedHatBoyState<Falling> {
    pub fn frame_name(&self) -> &str {
      FALLING_FRAME_NAME
    }

    pub fn sleep(self) -> RedHatBoyState<KnockedOut> {
      RedHatBoyState {
        context: self.context,
        _state: KnockedOut,
      }
    }

    pub fn update(mut self) -> FallingEndState {
      self.context = self.context.update(FALLING_FRAMES);
      if self.context.frame >= FALLING_FRAMES {
        FallingEndState::Complete(self.sleep())
      } else {
        FallingEndState::Falling(self)
      }
    }
  }

  impl RedHatBoyState<Idle> {
    pub fn frame_name(&self) -> &str {
      IDLE_FRAME_NAME
    }

    pub fn new(
      audio: Audio,
      jump_sound: Sound,
    ) -> Self {
      RedHatBoyState {
        context: RedHatBoyContext {
          audio,
          frame: 0,
          jump_sound,
          position: Point {
            x: STARTING_POINT,
            y: FLOOR,
          },
          velocity: Point {
            x: 0,
            y: 0,
          },
        },
        _state: Idle {},
      }
    }

    pub fn run(self) -> RedHatBoyState<Running> {
      RedHatBoyState {
        context: self.context.reset_frame().run_right(),
        _state: Running {},
      }
    }

    pub fn update(mut self) -> Self {
      self.context = self.context.update(IDLE_FRAMES);
      self
    }
  }

  impl RedHatBoyState<Jumping> {
    pub fn frame_name(&self) -> &str {
      JUMP_FRAME_NAME
    }

    pub fn knock_out(self) -> RedHatBoyState<Falling> {
      RedHatBoyState {
        context: self.context.reset_frame().stop(),
        _state: Falling,
      }
    }

    pub fn land_on(
      self,
      position: i16,
    ) -> RedHatBoyState<Running> {
      log!("land_on");
      RedHatBoyState {
        context: self.context.reset_frame().set_on(position),
        _state: Running,
      }
    }

    pub fn update(mut self) -> JumpingEndState {
      self.context = self.context.update(JUMPING_FRAMES);
      if self.context.position.y >= FLOOR {
        JumpingEndState::Landing(self.land_on(HEIGHT))
      } else {
        JumpingEndState::Jumping(self)
      }
    }
  }

  impl RedHatBoyState<KnockedOut> {
    pub fn frame_name(&self) -> &str {
      FALLING_FRAME_NAME
    }

    pub fn update(mut self) -> Self {
      self.context.frame = FALLING_FRAMES - 1;
      self.context = self.context.update(FALLING_FRAMES);
      self
    }
  }

  impl RedHatBoyState<Running> {
    pub fn frame_name(&self) -> &str {
      RUN_FRAME_NAME
    }

    pub fn jump(self) -> RedHatBoyState<Jumping> {
      RedHatBoyState {
        context: self
          .context
          .reset_frame()
          .set_vertical_velocity(JUMP_SPEED)
          .play_jump_sound(),
        _state: Jumping {},
      }
    }

    pub fn knock_out(self) -> RedHatBoyState<Falling> {
      RedHatBoyState {
        context: self.context.reset_frame().stop(),
        _state: Falling {},
      }
    }

    pub fn land_on(
      self,
      position: i16,
    ) -> RedHatBoyState<Running> {
      RedHatBoyState {
        context: self.context.set_on(position),
        _state: Running {},
      }
    }

    pub fn slide(self) -> RedHatBoyState<Sliding> {
      RedHatBoyState {
        context: self.context.reset_frame(),
        _state: Sliding {},
      }
    }

    pub fn update(mut self) -> Self {
      self.context = self.context.update(RUNNING_FRAMES);
      self
    }
  }

  impl RedHatBoyState<Sliding> {
    pub fn frame_name(&self) -> &str {
      SLIDING_FRAME_NAME
    }

    pub fn knock_out(self) -> RedHatBoyState<Falling> {
      RedHatBoyState {
        context: self.context.reset_frame().stop(),
        _state: Falling,
      }
    }

    pub fn land_on(
      self,
      position: i16,
    ) -> RedHatBoyState<Sliding> {
      log!("land_on sliding");
      RedHatBoyState {
        context: self.context.set_on(position),
        _state: Sliding {},
      }
    }

    pub fn stand(self) -> RedHatBoyState<Running> {
      RedHatBoyState {
        context: self.context.reset_frame(),
        _state: Running,
      }
    }

    pub fn update(mut self) -> SlidingEndState {
      log!("update sliding");
      self.context = self.context.update(SLIDING_FRAMES);
      if self.context.frame >= SLIDING_FRAMES {
        SlidingEndState::Complete(self.stand())
      } else {
        SlidingEndState::Sliding(self)
      }
    }
  }

  pub enum FallingEndState {
    Complete(RedHatBoyState<KnockedOut>),
    Falling(RedHatBoyState<Falling>),
  }

  pub enum JumpingEndState {
    Jumping(RedHatBoyState<Jumping>),
    Landing(RedHatBoyState<Running>),
  }

  pub enum SlidingEndState {
    Complete(RedHatBoyState<Running>),
    Sliding(RedHatBoyState<Sliding>),
  }
}

fn rightmost(obstacle_list: &[Box<dyn Obstacle>]) -> i16 {
  obstacle_list
    .iter()
    .map(|obstacle| obstacle.right())
    .max_by(|x, y| x.cmp(y))
    .unwrap_or(0)
}
