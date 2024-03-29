//! Author: @justmike2000
//! Repo: https://github.com/justmike2000/item_wars/

use ggez::event::{KeyCode, KeyMods};
use ggez::{event, graphics, Context, GameResult};
use graphics::{GlBackendSpec, ImageGeneric, Rect};
use glam::*;

use std::sync::{Arc, Mutex};
use std::{ops::Index, time::{Duration, Instant}};
use std::path;
use std::env;
use std::collections::HashMap;
use std::io::{self};
use std::net::{UdpSocket, SocketAddr};

use serde::{Deserialize, Serialize};
use clap::App;
use rand::Rng;
use serde_json::*;
use crossbeam_channel::bounded;
use bytes::Bytes;

// The first thing we want to do is set up some constants that will help us out later.

const SCREEN_SIZE: (f32, f32) = (640.0, 480.0);
const GRID_CELL_SIZE: f32 = 32.0;

const MAX_PLAYERS: usize = 2;

const PLAYER_MAX_HP: i64 = 100;
const PLAYER_MAX_MP: i64 = 30;
const PLAYER_MAX_STR: i64 = 10;
const PLAYER_MOVE_SPEED: f32 = 1.0;
const PLAYER_TOP_ACCEL_SPEED: f32 = 5.0;
const PLAYER_ACCEL_SPEED: f32 = 0.2;
const PLAYER_STARTING_ACCEL: f32 = 0.4;
const PLAYER_JUMP_HEIGHT: f32 = 0.5;
const PLAYER_CELL_HEIGHT: f32 = 44.0;
const PLAYER_CELL_WIDTH: f32 = 34.0;

const POTION_WIDTH: f32 = 42.0;
const POTION_HEIGHT: f32 = 42.0;

const MAP_CURRENT_FRICTION: f32 = 5.0;

const UPDATES_PER_SECOND: f32 = 60.0;
const DRAW_MILLIS_PER_UPDATE: u64 = (1.0 / UPDATES_PER_SECOND * 1000.0) as u64; 
const NET_MILLIS_PER_UPDATE: u64 = 1; // 20 ticks

// checks
const NET_GAME_START_CHECK_MILLIS: u64 = 500;
const NET_GAME_READY_CHECK: u64 = 100;


#[derive(PartialOrd, Clone, Copy, Debug, Serialize, Deserialize)]
struct Position {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl From<Position> for Rect {
    fn from(pos: Position) -> Self {
        Rect { x: pos.x, y: pos.y, w: pos.w, h: pos.h }
    }
}

impl PartialEq for Position {
    fn eq(&self, other: &Self) -> bool {
        Rect::from(*self).overlaps(&Rect::from(*other))
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
struct Direction {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
}

impl From<Direction> for f32 {
    fn from(dir: Direction) -> f32 {
        if dir.up {
            0.0
        } else if dir.down {
            1.0
        } else if dir.left {
            2.0
        } else {
            3.0
        }
    }
}

impl From<f32> for Direction {
    fn from(item: f32) -> Self {
        let error_margin = 1.0;
        if item == 0.0 {
            Direction { up: true, down: false, left: false, right: false }
        } else if (item - 1.0).abs() < error_margin {
            Direction { up: false, down: true, left: false, right: false }
        } else if (item - 2.0).abs() < error_margin {
            Direction { up: false, down: false, left: true, right: false }
        } else {
            Direction { up: false, down: false, left: false, right: true }
        }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
enum PotionType {
    Health,
    Mana
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Potion {
    pos: Position,
    potion_type: PotionType,
    #[serde(skip_serializing, skip_deserializing)]
    texture: Option<ImageGeneric<GlBackendSpec>>,
}

impl Potion {

    pub fn new(pos: Position, potion_type: PotionType, texture: ImageGeneric<GlBackendSpec>) -> Self {
        Potion {
            pos,
            potion_type,
            texture: Some(texture),
        }
    }

    fn draw(&self, ctx: &mut Context) -> GameResult<()> {

        //let black_rectangle = graphics::Mesh::new_rectangle(
        //    ctx,
        //    graphics::DrawMode::fill(),
        //    Rect::new(self.pos.x, self.pos.y, self.pos.w, self.pos.h),
        //    [0.0, 0.0, 0.0, 1.0].into(),
        //)?;
        //graphics::draw(ctx, &black_rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))?;

        //let color = [0.0, 0.0, 1.0, 1.0].into();
        //let rectangle =
        //    graphics::Mesh::new_rectangle(ctx, graphics::DrawMode::fill(), self.pos.into(), color)?;
        //graphics::draw(ctx, &rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))
        let potion_frame = if self.potion_type == PotionType::Health {
            0.0
        } else if self.potion_type == PotionType::Mana {
            0.33
        } else {
            0.0
        };
        let param = graphics::DrawParam::new()
        .src(graphics::Rect {x: 0.0, y: potion_frame, w: 0.33, h: 0.33})
        .dest(Vec2::new(self.pos.x, self.pos.y))
        //.offset(Vec2::new(0.15, 0.0))
        .scale(Vec2::new(0.25, 0.25));
        //.rotation((time % cycle) as f32 / cycle as f32 * 6.28)
        //.offset(Vec2::new(150.0, 150.0));
        graphics::draw(ctx, &self.texture.clone().unwrap(), param)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Player {
    /// First we have the body of the player, which is a single `Segment`.
    body: Position,
    is_hit: bool,
    /// Then we have the current direction the player is moving. This is
    /// the direction it will move when `update` is called on it.
    dir: Direction,
    last_dir: Direction,
    ate: Option<Potion>,
    /// Store the direction that will be used in the `update` after the next `update`
    /// This is needed so a user can press two directions (eg. left then up)
    /// before one `update` has happened. It sort of queues up key press input
    name: String,
    hp: i64,
    mp: i64,
    str: i64,
    current_accel: f32,
    jumping: bool,
    jump_offset: f32,
    ready: bool,
    jump_direction: bool, // true up false down
    #[serde(skip_serializing, skip_deserializing)]
    texture: Option<ImageGeneric<GlBackendSpec>>,
    animation_frame: f32,
    animation_total_frames: f32,
    #[serde(skip_serializing, skip_deserializing)]
    last_animation: Option<std::time::Instant>,
    animation_duration: std::time::Duration,
}

impl Player {
    pub fn new(name: String, pos: Position, texture: Option<ImageGeneric<GlBackendSpec>>) -> Self {
        // Our player will initially have a body and one body segment,
        // and will be moving to the right.
        Player {
            name,
            body: pos,
            dir: Direction::default(),
            last_dir: Direction::default(),
            ate: None,
            current_accel: PLAYER_STARTING_ACCEL,
            hp: PLAYER_MAX_HP,
            mp: PLAYER_MAX_MP,
            str: PLAYER_MAX_STR,
            texture,
            jumping: false,
            jump_offset: 0.0,
            jump_direction: true,
            ready: false,
            animation_frame: 0.0,
            animation_total_frames: 4.0,
            last_animation: Some(std::time::Instant::now()),
            animation_duration:  Duration::new(0, 150_000_000),
            is_hit: false,
        }
    }

    //fn eats(&self, potion: &Potion) -> bool {
    //    if self.body == potion.pos {
    //        true
    //    } else {
    //        false
    //    }
    //}

    fn reset_last_dir(&mut self) {
        self.last_dir.left = false;
        self.last_dir.right = false;
        self.last_dir.up = false;
        self.last_dir.down = false;
    }

    fn move_direction(&mut self) {
        self.reset_last_dir();
        if self.current_accel < PLAYER_TOP_ACCEL_SPEED {
            self.current_accel += PLAYER_ACCEL_SPEED;
        }
        if self.dir.up && self.body.y > PLAYER_CELL_HEIGHT {
            self.body.y -= PLAYER_MOVE_SPEED + self.current_accel;
            self.last_dir.up = true;
        }
        if self.dir.down && self.body.y < SCREEN_SIZE.1 - (PLAYER_CELL_HEIGHT * 2.0) {
            self.body.y += PLAYER_MOVE_SPEED + self.current_accel;
            self.last_dir.down = true;
        }
        if self.dir.left && self.body.x > 0.0 {
            self.body.x -= PLAYER_MOVE_SPEED + self.current_accel;
            self.last_dir.left = true;
        }
        if self.dir.right && self.body.x < SCREEN_SIZE.0 - PLAYER_CELL_WIDTH {
            self.body.x += PLAYER_MOVE_SPEED + self.current_accel;
            self.last_dir.right = true;
        }
    }

    fn move_direction_cooldown(&mut self) {
        if self.last_dir.up && self.body.y > PLAYER_CELL_HEIGHT {
            self.body.y -= PLAYER_MOVE_SPEED + self.current_accel;
        }
        if self.last_dir.down && self.body.y < SCREEN_SIZE.1 - (PLAYER_CELL_HEIGHT * 2.0) {
            self.body.y += PLAYER_MOVE_SPEED + self.current_accel;
        }
        if self.last_dir.left && self.body.x > 0.0 {
            self.body.x -= PLAYER_MOVE_SPEED + self.current_accel;
        }
        if self.last_dir.right && self.body.x < SCREEN_SIZE.0 - PLAYER_CELL_WIDTH {
            self.body.x += PLAYER_MOVE_SPEED + self.current_accel;
        }
        if self.current_accel > 0.0 {
            self.current_accel -= PLAYER_ACCEL_SPEED * MAP_CURRENT_FRICTION;
        }
    }

    fn is_moving(&self) -> bool {
        self.dir.up || self.dir.down || self.dir.left || self.dir.right
    }

    fn update(&mut self, do_move: bool) {
        if self.jumping {
            if self.jump_direction && self.jump_offset < PLAYER_JUMP_HEIGHT {
                self.jump_offset += 0.1;
            } else if self.jump_direction && self.jump_offset == PLAYER_JUMP_HEIGHT {
                self.jump_direction = false;
            } else if !self.jump_direction && self.jump_offset <= PLAYER_JUMP_HEIGHT && self.jump_offset > 0.0 {
                self.jump_offset -= 0.1;
            } else {
                self.jumping = false;
                self.jump_offset = 0.0;
                self.jump_direction = true;
            }
        } else {
            self.jump_offset = 0.0;
        }
        if do_move {
            if self.is_moving() {
                self.move_direction()
            } else if self.current_accel > PLAYER_STARTING_ACCEL {
                self.move_direction_cooldown()
            }
        }
        //if self.eats(food) && !self.jumping {
        //    self.ate = Some(food.clone());
        //} else {
        //    self.ate = None
        //}
    }

    fn get_animation_direction(&self) -> f32 {
        if self.dir.up {
            0.25
        } else if self.dir.left {
            0.5
        } else if self.dir.right {
            0.75
        } else if self.dir.down {
            0.0
        } else if self.last_dir.left {
            0.5
        } else if self.last_dir.right {
           0.75
        } else if self.last_dir.up {
            0.25
        } else {
            0.0
        }
    }

    fn animate_frames(&mut self) {
        // Animation movement
        if self.is_moving() && self.last_animation.unwrap().elapsed() > self.animation_duration {
            self.last_animation = Some(Instant::now());
            self.animation_frame += 1.0 / self.animation_total_frames;
            if self.animation_frame >= 1.0 {
                self.animation_frame = 0.0;
            }
        }
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult<()> {
        if let Some(ate) = &self.ate {
            println!("{:?}", ate.pos);
        }
        // And then we do the same for the head, instead making it fully red to distinguish it.
        //let bounding_box_rectangle = graphics::Mesh::new_rectangle(
        //    ctx,
        //    graphics::DrawMode::fill(),
        //    self.body.into(),
        //    [1.0, 0.5, 0.0, 1.0].into(),
        //)?;
        //graphics::draw(ctx, &bounding_box_rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))?;
        //let black_rectangle = graphics::Mesh::new_rectangle(
        //    ctx,
        //    graphics::DrawMode::fill(),
        //    Rect::new(self.body.x, self.body.y, self.body.w, self.body.h),
        //    [0.0, 0.0, 0.0, 1.0].into(),
        //)?;
        //graphics::draw(ctx, &black_rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))?;

        if self.jumping {
            let bounding_box_rectangle = graphics::Mesh::new_circle(
                ctx,
                graphics::DrawMode::fill(),
                ggez::mint::Point2 { x: self.body.x + 15.0,  y: self.body.y + 47.0 },
                14.0,
                1.0,
                graphics::Color::new(0.0, 0.0, 0.0, 0.3),
            )?;
            graphics::draw(ctx, &bounding_box_rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))?;
        }

        let black_rectangle = graphics::Mesh::new_rectangle(
            ctx,
            graphics::DrawMode::fill(),
            Rect::new(self.body.x - 13.0, self.body.y - 45.0, 60.0, 35.0),
            [0.0, 0.0, 0.0, 1.0].into(),
        )?;
        graphics::draw(ctx, &black_rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))?;

        let player_name = graphics::Text::new(graphics::TextFragment {
            text: self.name.clone(),
            color: Some(graphics::Color::new(1.0, 1.0, 1.0, 1.0)),
            // `Font` is a handle to a loaded TTF, stored inside the `Context`.
            // `Font::default()` always exists and maps to DejaVuSerif.
            font: Some(graphics::Font::default()),
            scale: Some(graphics::PxScale { x: 15.0, y: 15.0 }),
        });
        let player_hp = graphics::Text::new(graphics::TextFragment {
            text: format!("{}", self.hp),
            color: Some(graphics::Color::new(0.9, 0.0, 0.0, 1.0)),
            // `Font` is a handle to a loaded TTF, stored inside the `Context`.
            // `Font::default()` always exists and maps to DejaVuSerif.
            font: Some(graphics::Font::default()),
            scale: Some(graphics::PxScale { x: 15.0, y: 15.0 }),
        });
        let player_mp = graphics::Text::new(graphics::TextFragment {
            text: format!("{}", self.mp),
            color: Some(graphics::Color::new(0.0, 0.4, 1.0, 1.0)),
            // `Font` is a handle to a loaded TTF, stored inside the `Context`.
            // `Font::default()` always exists and maps to DejaVuSerif.
            font: Some(graphics::Font::default()),
            scale: Some(graphics::PxScale { x: 15.0, y: 15.0 }),
        });
        graphics::queue_text(ctx, &player_name, ggez::mint::Point2 { x: self.body.x - (self.name.chars().count() as f32) + 5.0, y: self.body.y - GRID_CELL_SIZE - 10.0 }, None);
        graphics::queue_text(ctx, &player_hp, ggez::mint::Point2 { x: self.body.x - (GRID_CELL_SIZE / 2.0) + 5.0, y: self.body.y - GRID_CELL_SIZE + 5.0 }, None);
        graphics::queue_text(ctx, &player_mp, ggez::mint::Point2 { x: self.body.x - (GRID_CELL_SIZE / 2.0) + 45.0, y: self.body.y - GRID_CELL_SIZE + 5.0 }, None);
        graphics::draw_queued_text(
            ctx,
            graphics::DrawParam::new()
                .dest(ggez::mint::Point2 { x: 0.0, y: 0.0}),
                //.rotation(-0.5),
            None,
            graphics::FilterMode::Linear,
        )?;
        self.animate_frames();
        let param = graphics::DrawParam::new()
        .src(graphics::Rect {x: self.animation_frame, y: self.get_animation_direction(), w: 0.25, h: 0.25})
        .dest(Vec2::new(self.body.x + 2.0, self.body.y - 10.0))
        .offset(Vec2::new(0.15, self.jump_offset))
        .scale(Vec2::new(0.1, 0.1));
        //.rotation((time % cycle) as f32 / cycle as f32 * 6.28)
        //.offset(Vec2::new(150.0, 150.0));
        if let Some(player_texture) = &self.texture {
            graphics::draw(ctx, player_texture, param)?;
        }
        Ok(())
    }
}

#[derive(Clone)]
struct Hud {
}

impl Hud {

    fn new() -> Hud {
        Hud {}
    }

    fn draw(&self, ctx: &mut Context, player: &Player) -> GameResult<()> {
        let color = [0.0, 0.0, 0.0, 1.0].into();
        let top_back = graphics::Rect {
                x: 0.0,
                y: 0.0,
                w: SCREEN_SIZE.0,
                h: GRID_CELL_SIZE,
        };
        let bottom_back = graphics::Rect {
                x: 0.0,
                y: SCREEN_SIZE.1 - GRID_CELL_SIZE,
                w: SCREEN_SIZE.0,
                h: GRID_CELL_SIZE,
        };
        let top_rectangle =
            graphics::Mesh::new_rectangle(ctx, graphics::DrawMode::fill(), top_back, color)?;
        graphics::draw(ctx, &top_rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))?;
        let bottom_rectangle =
            graphics::Mesh::new_rectangle(ctx, graphics::DrawMode::fill(), bottom_back, color)?;
        graphics::draw(ctx, &bottom_rectangle, (ggez::mint::Point2 { x: 0.0, y: 0.0 },))?;
        let player_name = graphics::Text::new(graphics::TextFragment {
                text: format!("Player: {}", player.name),
                color: Some(graphics::Color::new(1.0, 1.0, 1.0, 1.0)),
                // `Font` is a handle to a loaded TTF, stored inside the `Context`.
                // `Font::default()` always exists and maps to DejaVuSerif.
                font: Some(graphics::Font::default()),
                scale: Some(graphics::PxScale { x: 30.0, y: 30.0 }),
            });
        let hp_text = graphics::Text::new(graphics::TextFragment {
                text: format!("{}", player.hp),
                color: Some(graphics::Color::new(1.0, 0.2, 0.2, 1.0)),
                // `Font` is a handle to a loaded TTF, stored inside the `Context`.
                // `Font::default()` always exists and maps to DejaVuSerif.
                font: Some(graphics::Font::default()),
                scale: Some(graphics::PxScale { x: 30.0, y: 30.0 }),
            });
        let str_text = graphics::Text::new(graphics::TextFragment {
                text: format!("{}", player.str),
                color: Some(graphics::Color::new(1.0, 1.0, 0.2, 1.0)),
                // `Font` is a handle to a loaded TTF, stored inside the `Context`.
                // `Font::default()` always exists and maps to DejaVuSerif.
                font: Some(graphics::Font::default()),
                scale: Some(graphics::PxScale { x: 30.0, y: 30.0 }),
            });
        let mp_text = graphics::Text::new(graphics::TextFragment {
                text: format!("{}", player.mp),
                color: Some(graphics::Color::new(0.0, 0.4, 1.0, 1.0)),
                // `Font` is a handle to a loaded TTF, stored inside the `Context`.
                // `Font::default()` always exists and maps to DejaVuSerif.
                font: Some(graphics::Font::default()),
                scale: Some(graphics::PxScale { x: 30.0, y: 30.0 }),
            });
        graphics::queue_text(ctx, &str_text, ggez::mint::Point2 { x: 130.0, y: SCREEN_SIZE.1 - GRID_CELL_SIZE }, None);
        graphics::queue_text(ctx, &mp_text, ggez::mint::Point2 { x: 70.0, y: SCREEN_SIZE.1 - GRID_CELL_SIZE }, None);
        graphics::queue_text(ctx, &hp_text, ggez::mint::Point2 { x: 0.0, y: SCREEN_SIZE.1 - GRID_CELL_SIZE }, None);
        graphics::queue_text(ctx, &player_name, ggez::mint::Point2 { x: 0.0, y: 0.0 }, None);
        graphics::draw_queued_text(
                ctx,
                graphics::DrawParam::new()
                    .dest(ggez::mint::Point2 { x: 0.0, y: 0.0}),
                    //.rotation(-0.5),
                None,
                graphics::FilterMode::Linear,
            )?;
        Ok(())
    }
}


#[derive(PartialEq, Debug)]
enum NetActions {
    Sendposition,
    Newgame,
    Listgames,
    Ready,
    Getworld,
    Joingame,
    Getopponent,
    GetopponentName,
    Unknown
}

impl NetActions {
    fn from_string(action: String) -> NetActions {
        if action == "sendposition" {
            NetActions::Sendposition
        } else if action == "newgame" {
            NetActions::Newgame
        } else if action == "listgames" {
            NetActions::Listgames
        } else if action == "ready" {
            NetActions::Ready
        } else if action == "getworld" {
            NetActions::Getworld
        } else if action == "getopponent" {
            NetActions::Getopponent
        } else if action == "joingame" {
            NetActions::Joingame
        } else if action == "getopponentname" {
            NetActions::GetopponentName
        } else {
            NetActions::Unknown
        }
    }

    fn from_usize(action: usize) -> NetActions {
        if action == 1 {
            NetActions::Sendposition
        } else if action == 2 {
            NetActions::Newgame
        } else if action == 3 {
            NetActions::Listgames
        } else if action == 4 {
            NetActions::Ready
        } else if action == 5 {
            NetActions::Getworld
        } else if action == 6 {
            NetActions::Getopponent
        } else if action == 7 {
            NetActions::Joingame
        } else if action == 8 {
            NetActions::GetopponentName
        } else {
            NetActions::Unknown
        }
    }
}

impl Into<usize> for NetActions {
    fn into(self) -> usize {
        if self == NetActions::Sendposition {
            1
        } else if self == NetActions::Newgame {
            2
        } else if self == NetActions::Listgames {
            3
        } else if self == NetActions::Ready {
            4
        } else if self == NetActions::Getworld {
            5
        } else if self == NetActions::Getopponent {
            6
        } else if self == NetActions::Joingame {
            7
        } else if self == NetActions::GetopponentName {
            8
        } else {
            0
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkedGame {
    players: Vec<Player>,
    session_id: String,
    started: bool,
    completed: bool,
}

impl NetworkedGame {

    pub fn new(game_id: String) -> NetworkedGame {
        //let my_uuid = Uuid::new_v4().to_string();

        NetworkedGame {
            players: vec![],
            session_id: game_id,
            started: false,
            completed: false
        }
    }
}

pub struct GameServer {
    hostname: String,
    games: Vec<NetworkedGame>,
    game_count: String,
}

impl GameServer {

    fn new(hostname: String) -> GameServer {
        GameServer {
            hostname,
            games: vec![],
            game_count: "0".to_string(),
        }
    }

    fn host(&mut self) {
        //let listener = TcpListener::bind(self.hostname.clone()).unwrap();
        let mut socket = UdpSocket::bind(self.hostname.clone()).unwrap();

         // threaded game checking one thread per game
        // if Instant::now() - last_server_update > Duration::from_millis(16) {
        //    for game in self.games.iter_mut() {
        //        if game.started {
        //           let mut cloned_mut_vec = game.players.clone();
        //           let mut player1 =  cloned_mut_vec.first_mut().unwrap().clone();
        //           let mut player2 =  cloned_mut_vec.last_mut().unwrap().clone();
        //           if player1.clone().body == player2.clone().body {
        //               player1.is_hit = true;
        //               player2.is_hit = true;
        //               //println!("{:?}", player1.last_dir);
        //               //println!("HIT");
        //           } else {
        //               player1.is_hit = false;
        //               player2.is_hit = false;
        //           }
        //        }   
        //    }
        //    last_server_update = Instant::now();
        //}

        let mut last_server_update = Instant::now();
        loop {
            let mut buf = [0; 65_000];
            let (amt, src) = socket.recv_from(&mut buf).unwrap();
            let result = String::from_utf8(buf.to_vec()).unwrap();
            self.handle_connection(result, &mut socket, src, amt);



        }
    }

    fn new_game(&mut self) -> String {
        let mut count = self.game_count.parse::<i32>().unwrap();
        count += 1;
        self.game_count = count.to_string();
        let game = NetworkedGame::new(self.game_count.clone());
        let session_id = game.clone().session_id;
        self.games.push(game.clone());
        let arc_game = Arc::new(Mutex::new(game));
        let shared_game = arc_game.clone();
        std::thread::spawn(move || {
            loop {
                let mut game = shared_game.lock().unwrap();
            }
        });
        session_id
    }

    fn handle_connection(&mut self, request: String, socket: &mut UdpSocket, addr: SocketAddr, amt: usize) {
        let keys: Vec<&str> = request[0..amt].split(':').into_iter().collect();
        let game_id = keys[0];
        let player = keys[1];
        let command = NetActions::from_usize(keys[2].parse::<i32>().unwrap() as usize);
        let meta = keys[3];

        match command {
            NetActions::Newgame => {
                let game_id = self.new_game();
                let _ = socket.send_to(game_id.as_bytes(), addr);
            },
            NetActions::Listgames => {
                let game_info: Vec<Vec<String>> = self.games.iter().filter(|game| !game.started ).map(|game| {
                    vec![game.session_id.clone(), game.players.len().to_string()]
                }).collect();

                let result = format!("{:?}", game_info);
                let _ = socket.send_to(result.as_bytes(), addr);
            },
            NetActions::Getworld => {
                if let Some(game) = self.games.iter().find(|g| g.session_id == game_id) {
                    let _ = socket.send_to(json!(game).to_string().as_bytes(), addr);
                } else {
                    println!("Invalid Game {}", game_id);
                }
            },
            NetActions::Joingame => {
                if let Some(game) = self.games.iter_mut().find(|g| g.session_id == game_id) {
                    if game.players.len() < MAX_PLAYERS {
                        let player_pos = if game.players.is_empty() {
                            Position { x: 100.0, y: 250.0, w: PLAYER_CELL_WIDTH, h: PLAYER_CELL_HEIGHT }
                        } else {
                            Position { x: 500.0, y: 250.0, w: PLAYER_CELL_WIDTH, h: PLAYER_CELL_HEIGHT }
                        };
                        let new_player = Player::new(player.to_string(), player_pos, None);
                        game.players.push(new_player);
                        if game.players.len() == MAX_PLAYERS {
                            println!("Starting game {}", game.session_id);
                            game.started = true;
                        }
                        let _ = socket.send_to(json!(game).to_string().as_bytes(), addr);
                    } else {
                        println!("game {:?} is full", game.session_id);
                    }
                } else {
                    println!("Invalid Game {}", game_id);
                }
            },
            NetActions::Ready => {
                if let Some(game) = self.games.iter_mut().find(|g| g.session_id == game_id) {
                    for game_player in  game.players.iter_mut() {
                        if game_player.name == player {
                            game_player.ready = true;
                        }
                    }
                    let ready = game.players.iter().filter(|p| p.ready).count() == 2;
                    let result = json!({"ready": ready});
                    let _ = socket.send_to(result.to_string().as_bytes(), addr);
                } else {
                    println!("Invalid Game {}", game_id);
                }
            },
            NetActions::Sendposition => {
                if let Some(game) = self.games.iter_mut().find(|g| g.session_id == game_id) {
                    if let Some(player) = game.players.iter_mut().find(|p| p.name == player) {
                        let update_player: Vec<f32> = serde_json::from_str(meta).unwrap();
                        player.body.x = update_player[0];
                        player.body.y = update_player[1];
                        player.dir = Direction::from(update_player[2]);
                        player.jumping = update_player[3] != 0.0;
                        player.animation_frame = update_player[4];
                        player.last_dir = Direction::from(update_player[5]);
                    }
                } else {
                    println!("Invalid Game {}", game_id);
                }
            },
            NetActions::GetopponentName => {
                if let Some(game) = self.games.iter_mut().find(|g| g.session_id == game_id) {
                    if let Some(player) = game.players.iter_mut().find(|p| p.name != player) {
                        let _ = socket.send_to(player.name.as_bytes(), addr);
                    }
                }
            },
            NetActions::Getopponent => {
                if let Some(game) = self.games.iter().find(|g| g.session_id == game_id) {
                    if let Some(player) = game.players.iter().find(|p| p.name != player) {
                        let result = json!({"opponent": vec![player.body.x,
                                                             player.body.y,
                                                             player.dir.clone().into(),
                                                             player.jumping as usize as f32,
                                                             player.current_accel,
                                                             player.animation_frame]});
                        let _ = socket.send_to(result.to_string().as_bytes(), addr);
                    } else {
                       println!("Invalid Player {}", player);
                    }
                } else {
                    println!("Invalid Game {}", game_id);
                }
            },
            _ => {
                let _ = socket.send_to("Invalid Command".as_bytes(), addr);
            }
        }
    }

    fn send_message(host: String, game_id: String, player: String, msg: String, meta: String, block: bool) -> Option<String> {
        let socket = UdpSocket::bind("0.0.0.0:0").unwrap();
        socket.set_nonblocking(!block).unwrap();
        let _ = socket.connect(host);

        let net_action: usize = NetActions::from_string(msg).into();
        let msg = format!("{}:{}:{}:{}", game_id, player, net_action, meta);

        match socket.send(&Bytes::from(msg)) {
            Ok(_) => (),
            Err(_e) => {
                return None
            }
        }
    
        if !block {
            return Some("".to_string())
        }
        match socket.set_read_timeout(Some(Duration::new(1, 0))) {
            Ok(_) => (),
            Err(_e) => {
                return None
            }
        }
        let mut buf = [0; 5_000];
        match socket.recv(&mut buf) {
            Ok(size) => Some(String::from_utf8_lossy(&buf[0..size]).to_string()),
            Err(_e) => {
                None
            }
        }
    }
}

#[derive(Clone)]
struct GameState {
    player: Player,
    opponent: Player,
    food: Potion,
    server: String,
    game_id: String,
    started: bool,
    ready: bool,
    gameover: bool,
    last_draw_update: Instant,
    last_net_update: Instant,
    last_pos_send: Instant,
    last_ready_check: Instant,
    last_recv: Instant,
    hud: Hud,
    textures: HashMap<String, graphics::ImageGeneric<GlBackendSpec>>,
    player_receiver: crossbeam_channel::Receiver<Vec<f32>>,
    player_pos_sender: crossbeam_channel::Sender<Player>,
    opponent_positions: Vec<(f32, f32, f32, Instant)>,
}

impl GameState {

    fn join_game(host: String, player: String, game_id: String) -> String {
        let msg = "joingame".to_string();
        GameServer::send_message(host, game_id, player, msg, "".to_string(), true).unwrap()
    }

    fn send_ready(server: String, player: String, game_id: String) -> String {
        let msg = "ready".to_string();
        GameServer::send_message(server, game_id, player, msg, "".to_string(), true).unwrap()
    }

    fn get_opponent(server: String, player: String, game_id: String) -> Option<Vec<f32>> {
        let result = match GameServer::send_message(server, game_id, player, "getopponent".to_string(), "".to_string(), true) {
            Some(r) => r,
            None => {
                return None;
            },
        };
        if let Ok(opponent) = serde_json::from_str::<serde_json::Value>(&result) {
            if let Some(opponent_array) = opponent["opponent"].as_array() {
                let opponent_vec: Vec<f32> = opponent_array.iter().map(|p| p.as_f64().unwrap() as f32 ).collect();
                return Some(opponent_vec);
       
            }
        }
        None
    }

    fn get_opponent_name(server: String, player: String, game_id: String) -> String {
        let msg = "getopponentname".to_string();
        GameServer::send_message(server, game_id, player, msg, "".to_string(), true).unwrap()
    }

    fn get_world_state(server: String, player: String, game_id: String) -> Option<NetworkedGame> {
        let msg = "getworld".to_string();
        let result = GameServer::send_message(server, game_id, player, msg, "".to_string(), true).unwrap();
        match serde_json::from_str(&result) {
            Ok(r) => Some(r),
            Err(e) => {
                println!("Error in getting world: {} {}", e, result);
                None
            }
        }
    }

    fn send_position(server: String, player: Player, game_id: String) {
        let meta_position = vec![player.body.x, player.body.y, player.dir.into(), player.jumping as u8 as f32, player.animation_frame, player.last_dir.into()];
        GameServer::send_message(server, game_id, player.name, "sendposition".to_string(), json!(meta_position).to_string(), false);
    }

    pub fn new(player_name: String, host: String, game_id: String ,mut textures: HashMap<String, graphics::ImageGeneric<GlBackendSpec>>) -> Self {
        let result = GameState::join_game(host.clone(), player_name.clone(), game_id.clone());
        let game_state: NetworkedGame = serde_json::from_str(&result).unwrap();

        let mut rng = rand::thread_rng();
        let mut player_pos = Position { x: 100.0, y: 100.0, w: PLAYER_CELL_WIDTH, h: PLAYER_CELL_HEIGHT };
        let mut opponent_pos = Position { x: 100.0, y: 100.0, w: PLAYER_CELL_WIDTH, h: PLAYER_CELL_HEIGHT };
        let food_pos = Position { x: rng.gen_range(0, SCREEN_SIZE.0 as i16) as f32,
                                           y: rng.gen_range(0, SCREEN_SIZE.1 as i16) as f32,
                                           w: POTION_WIDTH,
                                           h: POTION_HEIGHT };
        let potion_texture = textures.remove("potion").unwrap();
        let player_texture = textures.remove("hero").unwrap();
        for game_state_player in game_state.players.iter() {
            if game_state_player.name != player_name.clone() {
                opponent_pos.x = game_state_player.body.x;
                opponent_pos.y = game_state_player.body.y;
            } else {
                player_pos.x = game_state_player.body.x;
                player_pos.y = game_state_player.body.y;
            }
        }
        let player = Player::new(player_name, player_pos, Some(player_texture.clone()));
        let opponent = Player::new("".to_string(), opponent_pos, Some(player_texture));

        let (s, r) = bounded(1);
        let (player_pos_sender, player_pos_receiver) = bounded(1);

        let game_state = GameState {
            player: player.clone(),
            opponent,
            server: host.clone(),
            game_id: game_id.clone(),
            food: Potion::new(food_pos, PotionType::Health, potion_texture),
            hud: Hud::new(),
            gameover: false,
            started: false,
            last_draw_update: Instant::now(),
            last_net_update: Instant::now(),
            last_pos_send: Instant::now(),
            last_ready_check: Instant::now(),
            last_recv: Instant::now(),
            ready: false,
            textures,
            player_receiver: r,
            player_pos_sender,
            opponent_positions: vec![],
        };

        let threaded_host_pos = host.clone();
        let threaded_game_id = game_id.clone();

        std::thread::spawn(move || {
            loop {
                match player_pos_receiver.recv() {
                    Ok(msg) => {
                        GameState::send_position(threaded_host_pos.clone(), msg.clone(), threaded_game_id.clone());
                    },
                    Err(_e_) => {

                    }
                }
            }
        });
        std::thread::spawn(move || {
            let mut last_net_update = Instant::now();
            loop {
                if Instant::now() - last_net_update >= Duration::from_millis(NET_MILLIS_PER_UPDATE) {
                   
                    //let get_world = GameState::get_world_state(threaded_host.clone(), threaded_player.name.clone(), game_id.clone()).unwrap();
                    if let Some(opponent) = GameState::get_opponent(host.clone(), player.name.clone(), game_id.clone()) {
                        match s.send(opponent) {
                            Ok(_) => (),
                            Err (e) => {
                                println!("{:?}", e);
                            },
                        }
                    }
                    last_net_update = Instant::now();
                }
            }
        });
        game_state
    }
}

impl event::EventHandler for GameState {
    fn update(&mut self, _ctx: &mut Context) -> GameResult {
        if !self.started {
            if Instant::now() - self.last_net_update >= Duration::from_millis(NET_GAME_START_CHECK_MILLIS) {
                let get_world = GameState::get_world_state(self.server.clone(), self.player.name.clone(), self.game_id.clone()).unwrap();
                if !get_world.started {
                    println!("Waiting for game {} to start...", self.game_id.clone());
                    self.last_net_update = Instant::now();
                    return Ok(())
                } else {
                    // Get opponent name
                    let opponent_name = GameState::get_opponent_name(self.server.clone(), self.player.name.clone(), self.game_id.clone());
                    self.opponent.name = opponent_name;
                    println!("Game started!");
                    self.started = true
                }
            } else {
                return Ok(())
            }
        } 

        // Get opponent
            if let Ok(net_opponent) = self.player_receiver.try_recv() {
                self.opponent.body.x = net_opponent[0];
                self.opponent.body.y = net_opponent[1];
                self.opponent.dir = Direction::from(net_opponent[2]);
                self.opponent.jumping = net_opponent[3] != 0.0;
                self.opponent.current_accel = net_opponent[4];
                if self.opponent_positions.len() < 2 {
                    self.opponent_positions.push((self.opponent.body.x, self.opponent.body.y, f32::from(self.opponent.dir.clone()), Instant::now()));
                } else {
                    self.opponent_positions.remove(0);
                    self.opponent_positions.push((self.opponent.body.x, self.opponent.body.y, f32::from(self.opponent.dir.clone()), Instant::now()));
                }
                //self.opponent.update();
                self.last_recv = Instant::now();
            } else if self.started && self.opponent_positions.len() == 2 {
                let opponent_times: Vec<Instant> = self.opponent_positions.iter().map(|y| y.3).collect();
                let time_difference = *opponent_times.index(1) - *opponent_times.index(0);

                if Instant::now() - self.last_recv > time_difference {
                    //if Instant::now() - self.last_recv
                    // Try interpoliation
                    if self.opponent_positions.len() == 2 && self.opponent.is_moving() {
                        let opponent_x: Vec<f32> = self.opponent_positions.iter().map(|x| x.0).collect();
                        let opponent_y: Vec<f32> = self.opponent_positions.iter().map(|y| y.1).collect();
                        let _opponent_dir: Vec<f32> = self.opponent_positions.iter().map(|y| y.2).collect();

                        let mut change_x: f32  = opponent_x.index(1) / opponent_x.index(0);
                        if change_x > PLAYER_MOVE_SPEED {
                            change_x = PLAYER_MOVE_SPEED;
                        }
                        self.opponent.body.x *= change_x;

                        let mut change_y: f32  = opponent_y.index(1) / opponent_y.index(0);
                        if change_y > PLAYER_MOVE_SPEED {
                            change_y = PLAYER_MOVE_SPEED;
                        }
                        self.opponent.body.y *= change_y;

                        self.opponent.reset_last_dir();
                        self.opponent_positions.clear();
                    }
                }
            }

        // Countdown till all players read
        if !self.ready && Instant::now() - self.last_ready_check >= Duration::from_millis(NET_GAME_READY_CHECK) {
            let ready_result: serde_json::Value = serde_json::from_str(&GameState::send_ready(self.server.clone(), self.player.name.clone(), self.game_id.clone())).unwrap();
            if let Some(ready) = ready_result["ready"].as_bool() {
                self.ready = ready;
                if ready {
                    println!("Game ready!");
                }
                return Ok(())
            }
            self.last_ready_check = Instant::now();
            return Ok(())
        } else if !self.ready {
            return Ok(())
        }

        // Send pos
        if Instant::now() - self.last_draw_update >= Duration::from_millis(DRAW_MILLIS_PER_UPDATE) {
            if !self.gameover {
                self.player.update(true);
                self.opponent.update(false);
            }
            self.last_draw_update = Instant::now();
        }
        //if Instant::now() - self.last_pos_send >= Duration::from_millis(SEND_POS_MILLIS_PER_UPDATE) && (self.player.is_moving() || self.player.jumping) {
        if self.player.is_moving() || self.player.jumping {
            let _ = self.player_pos_sender.send(self.player.clone());
            //self.last_pos_send = Instant::now();
        }
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        graphics::clear(ctx, [0.0, 0.5, 0.0, 1.0].into());
        let param = graphics::DrawParam::new()
        .dest(Vec2::new(0.0, 0.0));
        graphics::draw(ctx, self.textures.get("background").unwrap(), param)?;

        // <TODO Load Map> //

        if self.ready {
            // Then we tell the player and the items to draw themselves
            self.opponent.draw(ctx)?;
            self.player.draw(ctx)?;
            //self.food.draw(ctx)?;
            self.hud.draw(ctx, &self.player)?;
        }
         
        graphics::present(ctx)?;
        ggez::timer::yield_now();
        Ok(())
    }

    fn key_up_event(
        &mut self,
        _ctx: &mut Context,
        keycode: KeyCode,
        _keymod: KeyMods,
    ) {
        match keycode {
            KeyCode::A => self.player.dir.left = false,
            KeyCode::D => self.player.dir.right = false,
            KeyCode::W => self.player.dir.up = false,
            KeyCode::S => self.player.dir.down = false,
            KeyCode::Escape => panic!("Escape!"),
            _ => ()
        };
    }

    /// key_down_event gets fired when a key gets pressed.
    fn key_down_event(
        &mut self,
        _ctx: &mut Context,
        keycode: KeyCode,
        _keymod: KeyMods,
        _repeat: bool,
    ) {
        match keycode {
            KeyCode::A => self.player.dir.left = true,
            KeyCode::D => self.player.dir.right = true,
            KeyCode::W => self.player.dir.up = true,
            KeyCode::S => self.player.dir.down = true,
            KeyCode::Space => {
                if !self.player.jumping {
                    self.player.jumping = true
                }
            },
            _ => ()
        };
    }
}

fn main() -> GameResult {

    let matches = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg("-h --host=[HOSTNAME:PORT] 'Set as server and assign hostname:port'")
        .arg("-l --list=[HOSTNAME:PORT] 'List all games on server'")
        .arg("-p --player=[NAME] 'Player Name'")
        .arg("-s --server=[HOSTNAME:PORT] 'Host to connect to'")
        .arg("-g --game=[GAMEID] 'GameID to join'")
        .get_matches();

    // if hosting
    if let Some(server) = matches.value_of("host") {
        let safe_server = server.to_string();
        std::thread::spawn(move || {
            let mut gameserver = GameServer::new(safe_server);
            gameserver.host();
        });
        //let mut server_input = String::new();
        println!("Started Item Wars Server on {}", server);
        let mut player = "".to_string();
        let mut game_id = "".to_string();
        loop {
            let mut server_input = "".to_string();
            println!("\nITEM WARS ENTER COMMAND :> ");
            let _ = io::stdin().read_line(&mut server_input);
            server_input.retain(|c| !c.is_whitespace());

            let command = server_input.to_ascii_lowercase().to_string();
            let cloned_command = command.clone();
            if command.len() >= 7 && command[0..7].to_string() == "setgame" {
                game_id = command[7..].to_string();
                println!("Game ID set to {}", game_id);
            } else if command.len() >= 9 && command[0..9].to_string() == "setplayer" {
                player = command[9..].to_string();
                println!("Playername set to {}", player);
            } else if command == "exit" {
                panic!("Exit");
            } else {
                let result = match GameServer::send_message(server.to_string(),
                                                      game_id.clone(), player.to_string(), command, "".to_string(), true) {
                    Some(r) => r,
                    None => {
                        println!("Command not found!");
                        continue
                    }
                };
                println!("RESULT: {}", result);
                if cloned_command.clone() == "newgame" {
                    game_id = result;
                    println!("Game ID set to {}", game_id);
                }
            }
        }
    } else if let Some(list) = matches.clone().value_of("list") {
       let games = GameServer::send_message(list.to_string(),
                                            "".to_string(), "".to_string(), "listgames".to_string(),
                                            "".to_string(), true);
       println!("{:?}", games);
       Ok(())
    } else {
        let player_name = matches.clone().value_of("player").unwrap_or("Player").to_string();
        if player_name.len() > 8 {
            panic!("Player name too long!  max 8 characters");
        }
        if !player_name.chars().all(|x| x.is_alphanumeric()) {
            panic!("Invalid player name character!")
        }
        let host = matches.clone().value_of("server").unwrap_or("localhost:7878").to_string();
        let game_id = match matches.clone().value_of("game") {
            Some(g ) => g.to_string(),
            None => {
                panic!("Please provide gameid.")
            },
        };
        let check_world_game = GameState::get_world_state(host.clone(), player_name.clone(), game_id.clone()).unwrap();
        if !check_world_game.started {
            for player in check_world_game.players.iter() {
                if player.name == player_name {
                    panic!("Game already has player of same name!");
                }
            }
        }

        let resource_dir = if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
            let mut path = path::PathBuf::from(manifest_dir);
            path.push("textures");
            path
        } else {
            path::PathBuf::from("./textures")
        };

        let (mut ctx, events_loop) = ggez::ContextBuilder::new("iterm wars", "Mitt Miles")
            .window_setup(ggez::conf::WindowSetup::default().title("Item Wars!"))
            .window_mode(ggez::conf::WindowMode::default().dimensions(SCREEN_SIZE.0, SCREEN_SIZE.1))
            .add_resource_path(resource_dir)
            .build()?;
        // To enable fullscreen
        //graphics::set_fullscreen(&mut ctx, ggez::conf::FullscreenType::True).unwrap();

        // Load our textures
        let mut textures: HashMap<String, ImageGeneric<GlBackendSpec>> = HashMap::new();
        textures.insert("background".to_string(), graphics::Image::new(&mut ctx, "/tile.png").unwrap());
        textures.insert("hero".to_string(), graphics::Image::new(&mut ctx, "/hero.png").unwrap());
        textures.insert("potion".to_string(), graphics::Image::new(&mut ctx, "/potion.png").unwrap());

        // Next we create a new instance of our GameState struct, which implements EventHandler
        let state = GameState::new(player_name, host, game_id, textures);
        // And finally we actually run our game, passing in our context and state.
        event::run(ctx, events_loop, state)
    }
}