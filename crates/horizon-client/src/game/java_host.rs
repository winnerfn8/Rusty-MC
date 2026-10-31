//! The game as the Java layer's host (docs/java-mod-runtime.md): what the Java API layer's
//! classes (`Level.getBlockState`, `LivingEntity.getHealth`, `ServerPlayer.sendSystemMessage`...)
//! read and change while a hook point runs, through the same paths as native mods' server
//! helpers (`native_server.rs`).
//!
//! Handles: a level by its dimension (0 the overworld, 1 the nether, 2 the end), a block state by
//! the game's state id, an entity by its id: a server's players by their entity id, mobs by
//! theirs, and -1 for the player whose state is in this `Game` (the acting guest on a server, the
//! local player otherwise). Only the level the hook point runs in is at hand: the others are
//! parked, and reading them answers nothing.
use super::{Dim, Game};
use horizon_client::{lan::data::component_nbt, text::Text};
use horizon_java::hooks::{EntityView, Host, Obj, Side};
use horizon_protocol::packets::play::{self, Clientbound as C};
use horizon_world::registry::BlockStateId;

/// The player whose state is in `Game`.
pub(super) const ACTING: i32 = -1;

fn dim_of(level: i32) -> Option<Dim> {
    match level {
        0 => Some(Dim::Overworld),
        1 => Some(Dim::Nether),
        2 => Some(Dim::End),
        _ => None,
    }
}

pub(super) fn level_of(dim: Dim) -> i32 {
    match dim {
        Dim::Overworld => 0,
        Dim::Nether => 1,
        Dim::End => 2,
    }
}

fn namespaced(name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

impl Game<'_> {
    /// The level this `Game` ticks now, as the Java layer names it.
    pub(super) fn java_level(&self) -> Obj {
        Obj::Level(level_of(self.dim))
    }

    /// The acting player's uuid, when this is a server with that player on.
    fn java_uuid(&self, id: i32) -> Option<u128> {
        if id == ACTING {
            return self.lan_acting_uuid;
        }
        let host = self.lan.as_ref()?;
        host.hub
            .guests
            .iter()
            .find(|g| g.placed && g.entity_id == id)
            .map(|g| g.uuid)
    }
}

impl Host for Game<'_> {
    fn side(&self) -> Side {
        if self.remote.is_some() {
            Side::Client
        } else {
            Side::Server
        }
    }

    fn tick_count(&self) -> i32 {
        self.ticks as i32
    }

    fn levels(&self) -> Vec<i32> {
        let mut levels: Vec<i32> = self.levels_to_tick().into_iter().map(level_of).collect();
        if !levels.contains(&level_of(self.dim)) {
            levels.insert(0, level_of(self.dim));
        }
        levels
    }

    fn current_level(&self) -> i32 {
        level_of(self.dim)
    }

    fn level_name(&self, level: i32) -> String {
        dim_of(level).map(|d| d.id().to_owned()).unwrap_or_default()
    }

    fn level_time(&self, _level: i32) -> [i64; 2] {
        [self.level.time, self.level.day_time]
    }

    fn block_state(&self, level: i32, pos: [i32; 3]) -> Option<u32> {
        (level == level_of(self.dim)).then(|| self.world.block(pos[0], pos[1], pos[2]).0)
    }

    fn set_block_state(&mut self, level: i32, pos: [i32; 3], state: u32) -> bool {
        if level != level_of(self.dim) || self.remote.is_some() {
            return false;
        }
        self.pack
            .assets
            .registry
            .state(BlockStateId(state))
            .is_some()
            && self
                .place_state(pos, BlockStateId(state), false)
                .unwrap_or(false)
    }

    fn state_block(&self, state: u32) -> String {
        self.pack
            .assets
            .registry
            .state(BlockStateId(state))
            .map(|s| namespaced(&s.name))
            .unwrap_or_default()
    }

    fn default_state(&self, block: &str) -> Option<u32> {
        let registry = self.pack.assets.registry;
        registry
            .default_state(block)
            .or_else(|| registry.default_state(block.trim_start_matches("minecraft:")))
            .map(|s| s.0)
    }

    fn players(&self) -> Vec<i32> {
        match &self.lan {
            Some(host) => host
                .hub
                .guests
                .iter()
                .filter(|g| g.placed)
                .map(|g| g.entity_id)
                .collect(),
            None if self.player.is_some() => vec![ACTING],
            None => Vec::new(),
        }
    }

    fn entity(&self, id: i32) -> Option<EntityView> {
        if id == ACTING {
            let p = self.player.as_ref()?;
            let health = p.health.health();
            return Some(EntityView {
                kind: "minecraft:player".into(),
                level: level_of(self.dim),
                pos: [p.position.x, p.position.y, p.position.z],
                health: Some([health, 20.0]),
                name: self.lan_acting_name.clone().unwrap_or_default(),
                alive: health > 0.0,
                player: true,
                creative: self.mode_id() == 1,
            });
        }
        if let Some(g) = self
            .lan
            .as_ref()
            .and_then(|h| h.hub.guests.iter().find(|g| g.placed && g.entity_id == id))
        {
            return Some(EntityView {
                kind: "minecraft:player".into(),
                level: level_of(Dim::from_id(&g.dimension)),
                pos: g.position,
                health: Some([g.health, 20.0]),
                name: g.name.clone(),
                alive: g.health > 0.0,
                player: true,
                creative: g.game_mode == 1,
            });
        }
        let m = self.mobs.iter().find(|m| m.id as i32 == id)?;
        Some(EntityView {
            kind: namespaced(&m.kind.id),
            level: level_of(self.dim),
            pos: [m.position.x, m.position.y, m.position.z],
            health: Some([m.health, horizon_client::mobs::finalize::max_health(m)]),
            name: m.custom_name.clone().unwrap_or_else(|| m.kind.id.clone()),
            alive: m.health > 0.0,
            player: false,
            creative: false,
        })
    }

    fn set_health(&mut self, id: i32, health: f32) -> bool {
        if self.remote.is_some() {
            return false;
        }
        let Some(m) = self.mobs.iter_mut().find(|m| m.id as i32 == id) else {
            return false;
        };
        let max = horizon_client::mobs::finalize::max_health(m);
        m.health = health.clamp(0.0, max);
        true
    }

    fn send_message(&mut self, player: i32, text: &str, overlay: bool) {
        let line = Text::literal(text);
        let Some(uuid) = self.java_uuid(player) else {
            // No server here: the local player's own chat.
            if player == ACTING && self.lan.is_none() && !overlay {
                self.system_message(line);
            }
            return;
        };
        if overlay {
            self.native_action_bar(uuid, line);
        } else if self.lan_acting_uuid == Some(uuid) {
            self.guest_packets.push(C::SystemChat(play::SystemChat {
                content: component_nbt(&line.component()),
                overlay: false,
            }));
        } else {
            self.native_tell(uuid, line);
        }
    }

    fn broadcast(&mut self, text: &str) {
        self.native_broadcast(Text::literal(text));
    }
}
