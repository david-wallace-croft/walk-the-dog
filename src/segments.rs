use std::rc::Rc;

use web_sys::HtmlImageElement;

use crate::{
  engine::{Image, Point, Rect, SpriteSheet},
  game::{Barrier, Obstacle, Platform},
};

const FIRST_PLATFORM: i16 = 400;
static FLOATING_PLATFORM_BOUNDING_BOXES: [Rect; 3] = [
  Rect {
    position: Point {
      x: 0,
      y: 0,
    },
    width: 60,
    height: 54,
  },
  Rect {
    position: Point {
      x: 60,
      y: 0,
    },
    width: 384 - (60 * 2),
    height: 93,
  },
  Rect {
    position: Point {
      x: 384 - 60,
      y: 0,
    },
    width: 60,
    height: 54,
  },
];
const FLOATING_PLATFORM_SPRITES: [&str; 3] = [
  "13.png", "14.png", "15.png",
];
const HEIGHT: i16 = 600;
const LOW_PLATFORM: i16 = 420;
const HIGH_PLATFORM: i16 = 375;
const INITIAL_STONE_OFFSET: i16 = 150;
const STONE_ON_GROUND: i16 = 546;

pub fn platform_and_stone(
  offset_x: i16,
  sprite_sheet: Rc<SpriteSheet>,
  stone: HtmlImageElement, // TODO: use Rc
) -> Vec<Box<dyn Obstacle>> {
  vec![
    Box::new(Barrier::new(Image::new(
      stone,
      Point {
        x: offset_x + INITIAL_STONE_OFFSET,
        y: STONE_ON_GROUND,
      },
    ))),
    Box::new(create_floating_platform(
      Point {
        x: offset_x + FIRST_PLATFORM,
        y: HIGH_PLATFORM,
      },
      sprite_sheet,
    )),
  ]
}

pub fn stone_and_platform(
  offset_x: i16,
  sprite_sheet: Rc<SpriteSheet>,
  stone: HtmlImageElement, // TODO: use Rc
) -> Vec<Box<dyn Obstacle>> {
  vec![
    Box::new(Barrier::new(Image::new(
      stone,
      Point {
        x: offset_x + INITIAL_STONE_OFFSET,
        y: STONE_ON_GROUND,
      },
    ))),
    Box::new(create_floating_platform(
      Point {
        x: offset_x + FIRST_PLATFORM,
        y: LOW_PLATFORM,
      },
      sprite_sheet,
    )),
  ]
}

// private functions

fn create_floating_platform(
  position: Point,
  sprite_sheet: Rc<SpriteSheet>,
) -> Platform {
  Platform::new(
    &FLOATING_PLATFORM_BOUNDING_BOXES,
    position,
    sprite_sheet,
    &FLOATING_PLATFORM_SPRITES,
  )
}
