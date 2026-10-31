//! Mixin hook points, the runtime side. A Java mod's `@Inject(method = "tickServer", at =
//! @At("HEAD"))` can't be woven into vanilla bytecode (there is none): it binds to the hook point
//! the Rust port of that method calls at its start or end. Each hook point is also a native hook
//! (`native_mixin_*` in `crates/horizon-client/src/game/native_hooks.rs`, named in
//! `native-hooks.json` with the vanilla method it sits in) that native mods hook at compile time;
//! this module is for handlers attached while the game runs.
//!
//! A port calls [`run`] with a [`Call`], the game as a [`Host`] and what the vanilla method is
//! handed (a [`Site`]: `this` and the arguments, made only when something is attached). At the
//! start, a handler may cancel the method or cancel it with a return value
//! (`CallbackInfo.cancel`, `CallbackInfoReturnable.setReturnValue`); at the end, it reads the
//! return value and may change it. The game's lifecycle (a server starting, stopping, a level's
//! tick) has hook points of its own, for Fabric API's events. Nothing attached, [`run`] is one
//! relaxed atomic load and a branch.
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

/// A hook point: one vanilla method's start (`Head`) or end (`Return`), in `native-hooks.json`'s
/// order, then the lifecycle points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Hook {
    /// `MinecraftServer.tickServer`, start: cancelling skips the whole server tick.
    TickServerHead,
    /// `MinecraftServer.tickServer`, end of a tick that didn't fail.
    TickServerReturn,
    /// `BlockBehaviour.getDestroyProgress`, start: a return value replaces the mining progress.
    GetDestroyProgressHead,
    /// `BlockBehaviour.getDestroyProgress`, end: the progress per tick, which a handler may change.
    GetDestroyProgressReturn,
    /// `Block.getDrops` (a block broken by a player), start: a return value replaces the drops.
    GetDropsHead,
    /// `Block.getDrops`, end: the drops, which a handler may change.
    GetDropsReturn,
    /// `Player.interactOn` (the entity under the crosshair), start: cancelling stops the interaction.
    InteractOnHead,
    /// `Player.interactOn`, end: whether the entity took the click.
    InteractOnReturn,
    /// A server starts opening its world (before any level loads).
    ServerStarting,
    /// A server's world is open and ticks next.
    ServerStarted,
    /// A server starts closing (before its players leave and the final save).
    ServerStopping,
    /// A server's world closed after its final save.
    ServerStopped,
    /// A level's part of a server tick starts (`ServerLevel.tick`); the [`Site`]'s `this` is it.
    LevelTickStart,
    /// A level's part of a server tick ended.
    LevelTickEnd,
}

impl Hook {
    pub const ALL: [Hook; 14] = [
        Hook::TickServerHead,
        Hook::TickServerReturn,
        Hook::GetDestroyProgressHead,
        Hook::GetDestroyProgressReturn,
        Hook::GetDropsHead,
        Hook::GetDropsReturn,
        Hook::InteractOnHead,
        Hook::InteractOnReturn,
        Hook::ServerStarting,
        Hook::ServerStarted,
        Hook::ServerStopping,
        Hook::ServerStopped,
        Hook::LevelTickStart,
        Hook::LevelTickEnd,
    ];

    /// The name: for a mixin hook point its catalog name (`native-hooks.json`), also the native
    /// hook's `native_<name>`; for a lifecycle point `event_<what>`.
    pub fn name(self) -> &'static str {
        match self {
            Hook::TickServerHead => "mixin_minecraft_server_tick_server_head",
            Hook::TickServerReturn => "mixin_minecraft_server_tick_server_return",
            Hook::GetDestroyProgressHead => "mixin_block_behaviour_get_destroy_progress_head",
            Hook::GetDestroyProgressReturn => "mixin_block_behaviour_get_destroy_progress_return",
            Hook::GetDropsHead => "mixin_block_get_drops_head",
            Hook::GetDropsReturn => "mixin_block_get_drops_return",
            Hook::InteractOnHead => "mixin_player_interact_on_head",
            Hook::InteractOnReturn => "mixin_player_interact_on_return",
            Hook::ServerStarting => "event_server_starting",
            Hook::ServerStarted => "event_server_started",
            Hook::ServerStopping => "event_server_stopping",
            Hook::ServerStopped => "event_server_stopped",
            Hook::LevelTickStart => "event_level_tick_start",
            Hook::LevelTickEnd => "event_level_tick_end",
        }
    }

    pub fn from_name(name: &str) -> Option<Hook> {
        Hook::ALL.into_iter().find(|h| h.name() == name)
    }

    /// Whether this is a vanilla method's start or end a mixin binds to (else a lifecycle point).
    pub fn is_mixin(self) -> bool {
        self.name().starts_with("mixin_")
    }

    /// Whether this is a method's start (else its end, or a lifecycle point).
    pub fn is_head(self) -> bool {
        self.name().ends_with("_head")
    }
}

/// Which side a game runs: the server (a server thread, a dedicated server) or a client of one.
/// Mixin configs' `client` and `server` lists, and Fabric's `client` and `server` entry points,
/// apply by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Server,
    Client,
}

/// A Java object a hook point hands a handler, as `this` or an argument, by handle: the Java API
/// layer makes the object, and its methods ask the [`Host`] about it.
#[derive(Clone, Debug, PartialEq)]
pub enum Obj {
    Null,
    /// The server (`MinecraftServer`).
    Server,
    /// A level, by the host's id (`ServerLevel`).
    Level(i32),
    /// An entity, by its id (`Entity`, `LivingEntity`, `ServerPlayer`... as the host describes it).
    Entity(i32),
    BlockPos([i32; 3]),
    /// A block state, by the host's state id (`BlockState`).
    BlockState(u32),
    /// The block of a block state, by the host's state id (`Block`).
    Block(u32),
    /// An item stack: the item's id and the count (`ItemStack`).
    Stack(String, i32),
    /// `InteractionHand`: the main hand (true) or the off hand.
    Hand(bool),
    /// A `BooleanSupplier` that answers true (`tickServer`'s `hasTimeLeft`).
    True,
}

/// What a vanilla method is handed where a hook point sits: `this` (`Obj::Null` for a static
/// method) and the arguments, in the method's order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Site {
    pub this: Option<Obj>,
    pub args: Vec<Obj>,
}

impl Site {
    pub fn new(this: Obj, args: Vec<Obj>) -> Self {
        Self {
            this: Some(this),
            args,
        }
    }

    /// A static method's site.
    pub fn of_static(args: Vec<Obj>) -> Self {
        Self { this: None, args }
    }
}

/// An entity as the Java API layer reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityView {
    /// Its type, `minecraft:zombie`.
    pub kind: String,
    pub level: i32,
    pub pos: [f64; 3],
    /// Health and maximum health, for a living entity.
    pub health: Option<[f32; 2]>,
    pub name: String,
    pub alive: bool,
    pub player: bool,
    pub creative: bool,
}

/// What the game offers Java mods while a hook point runs: the Java API layer's methods
/// (`Level.getBlockState`, `LivingEntity.getHealth`...) land here. Handles are the game's own
/// (level ids, entity ids, block state ids). Every method has a default that answers nothing,
/// so a host implements what it has.
pub trait Host {
    fn side(&self) -> Side {
        Side::Server
    }
    /// `MinecraftServer.getTickCount`.
    fn tick_count(&self) -> i32 {
        0
    }
    /// The levels that are loaded, by id.
    fn levels(&self) -> Vec<i32> {
        Vec::new()
    }
    /// The level the hook point runs in.
    fn current_level(&self) -> i32 {
        0
    }
    /// A level's dimension, `minecraft:overworld`.
    fn level_name(&self, _level: i32) -> String {
        String::new()
    }
    /// A level's game time and day time.
    fn level_time(&self, _level: i32) -> [i64; 2] {
        [0, 0]
    }
    fn block_state(&self, _level: i32, _pos: [i32; 3]) -> Option<u32> {
        None
    }
    /// Sets a block state (`Level.setBlock`'s neighbour updates and all); whether it changed.
    fn set_block_state(&mut self, _level: i32, _pos: [i32; 3], _state: u32) -> bool {
        false
    }
    /// A block state's block id, `minecraft:stone`.
    fn state_block(&self, _state: u32) -> String {
        String::new()
    }
    /// A block's default state, by its id.
    fn default_state(&self, _block: &str) -> Option<u32> {
        None
    }
    /// The players' entity ids.
    fn players(&self) -> Vec<i32> {
        Vec::new()
    }
    fn entity(&self, _id: i32) -> Option<EntityView> {
        None
    }
    fn set_health(&mut self, _id: i32, _health: f32) -> bool {
        false
    }
    /// A system message to one player, in chat or above the hotbar (`overlay`).
    fn send_message(&mut self, _player: i32, _text: &str, _overlay: bool) {}
    /// A system message to every player.
    fn broadcast(&mut self, _text: &str) {}
}

/// A host that has nothing: for hook points with no game at hand.
pub struct NoHost;

impl Host for NoHost {}

/// A vanilla method's callback, as a mixin handler sees it (`CallbackInfo`,
/// `CallbackInfoReturnable`).
#[derive(Debug, Clone, PartialEq)]
pub struct Call<R> {
    canceled: bool,
    value: Option<R>,
}

impl<R> Call<R> {
    /// At a method's start: not cancelled, no return value.
    pub fn head() -> Self {
        Self {
            canceled: false,
            value: None,
        }
    }

    /// At a method's end, holding what it returns.
    pub fn returning(value: R) -> Self {
        Self {
            canceled: false,
            value: Some(value),
        }
    }

    /// `CallbackInfo.cancel`: at the start, the method returns at once (with its type's default
    /// value, or the return value set).
    pub fn cancel(&mut self) {
        self.canceled = true;
    }

    /// `CallbackInfoReturnable.setReturnValue`: the method returns this. At the start it
    /// also cancels, as in Mixin.
    pub fn set_return_value(&mut self, value: R) {
        self.value = Some(value);
        self.canceled = true;
    }

    pub fn is_canceled(&self) -> bool {
        self.canceled
    }

    pub fn return_value(&self) -> Option<&R> {
        self.value.as_ref()
    }

    pub fn into_value(self) -> Option<R> {
        self.value
    }
}

/// A value as the Java layer passes it: the types a handler can read and set. Others pass as
/// [`Slot::Other`] and stay as they are.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Slot {
    /// No value: a void method, or a start before anything set one.
    None,
    Bool(bool),
    Float(f32),
    /// A type the Java layer can't hand over yet (a list of item stacks...).
    Other,
}

/// A return type a hook point carries.
pub trait Value: Sized {
    fn slot(&self) -> Slot;
    fn from_slot(slot: Slot) -> Option<Self>;
}

impl Value for () {
    fn slot(&self) -> Slot {
        Slot::None
    }
    fn from_slot(_: Slot) -> Option<Self> {
        None
    }
}

impl Value for bool {
    fn slot(&self) -> Slot {
        Slot::Bool(*self)
    }
    fn from_slot(slot: Slot) -> Option<Self> {
        match slot {
            Slot::Bool(b) => Some(b),
            _ => None,
        }
    }
}

impl Value for f32 {
    fn slot(&self) -> Slot {
        Slot::Float(*self)
    }
    fn from_slot(slot: Slot) -> Option<Self> {
        match slot {
            Slot::Float(f) => Some(f),
            _ => None,
        }
    }
}

impl<T> Value for Vec<T> {
    fn slot(&self) -> Slot {
        Slot::Other
    }
    fn from_slot(_: Slot) -> Option<Self> {
        None
    }
}

/// What runs the attached handlers of a hook point: it may set `canceled` and change the value,
/// and reach the game through the host.
pub type Runtime = Box<dyn Fn(Hook, &mut bool, &mut Slot, &Site, &mut dyn Host) + Send + Sync>;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
/// How many handlers each hook point has.
static ATTACHED: [AtomicU32; Hook::ALL.len()] = [const { AtomicU32::new(0) }; Hook::ALL.len()];

/// Installs what runs attached handlers, once per process (the Java layer). False when one is
/// installed already.
pub fn install(runtime: Runtime) -> bool {
    RUNTIME.set(runtime).is_ok()
}

/// One more handler on `hook`: [`run`] calls the runtime from now on.
pub fn attach(hook: Hook) {
    ATTACHED[hook as usize].fetch_add(1, Ordering::Relaxed);
}

/// One handler fewer on `hook`.
pub fn detach(hook: Hook) {
    let _ = ATTACHED[hook as usize]
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1));
}

pub fn attached(hook: Hook) -> u32 {
    ATTACHED[hook as usize].load(Ordering::Relaxed)
}

/// Runs the handlers attached to `hook` with `call`, the game as `host`, and `site` (what the
/// vanilla method is handed, made only when something is attached). With none attached: one
/// relaxed load and a branch, inlined at the call site.
#[inline(always)]
pub fn run<R: Value, H: Host>(
    hook: Hook,
    call: &mut Call<R>,
    host: &mut H,
    site: impl FnOnce(&H) -> Site,
) {
    if ATTACHED[hook as usize].load(Ordering::Relaxed) != 0 {
        let site = site(host);
        run_attached(hook, call, host, &site);
    }
}

/// A lifecycle point: [`run`] with nothing to cancel or return.
#[inline(always)]
pub fn event<H: Host>(hook: Hook, host: &mut H, site: impl FnOnce(&H) -> Site) {
    if ATTACHED[hook as usize].load(Ordering::Relaxed) != 0 {
        let site = site(host);
        run_attached(hook, &mut Call::<()>::head(), host, &site);
    }
}

#[cold]
#[inline(never)]
fn run_attached<R: Value>(hook: Hook, call: &mut Call<R>, host: &mut dyn Host, site: &Site) {
    let Some(runtime) = RUNTIME.get() else {
        return;
    };
    let before = call.value.as_ref().map_or(Slot::None, Value::slot);
    let mut slot = before;
    runtime(hook, &mut call.canceled, &mut slot, site, host);
    if slot != before
        && let Some(v) = R::from_slot(slot)
    {
        call.value = Some(v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_round_trip() {
        let mut names: Vec<&str> = Hook::ALL.iter().map(|h| h.name()).collect();
        for h in Hook::ALL {
            assert_eq!(Hook::from_name(h.name()), Some(h));
            assert_eq!(h.is_head(), h.name().ends_with("_head"));
            assert!(h.is_mixin() || h.name().starts_with("event_"), "{h:?}");
            assert!(!(h.is_head() && !h.is_mixin()));
        }
        names.sort();
        names.dedup();
        assert_eq!(names.len(), Hook::ALL.len());
        assert_eq!(Hook::from_name("tick"), None);
    }

    #[test]
    fn a_call_cancels_and_carries_its_return_value() {
        let mut c = Call::<f32>::head();
        assert!(!c.is_canceled());
        c.set_return_value(2.0);
        assert!(c.is_canceled());
        assert_eq!(c.into_value(), Some(2.0));
        let mut c = Call::returning(true);
        c.cancel();
        assert_eq!(c.return_value(), Some(&true));
    }

    /// Nothing attached, nothing runs; attached, the runtime cancels and changes values; a type it
    /// can't hand over stays as it is. (One test: the runtime is installed once per process.)
    #[test]
    fn the_runtime_runs_only_for_attached_hook_points() {
        use std::sync::atomic::AtomicUsize;
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let site = |_: &NoHost| Site::new(Obj::Server, vec![Obj::True]);
        let mut c = Call::<()>::head();
        run(Hook::TickServerHead, &mut c, &mut NoHost, site);
        assert!(install(Box::new(|hook, canceled, value, site, host| {
            CALLS.fetch_add(1, Ordering::Relaxed);
            assert_eq!(host.tick_count(), 0);
            match hook {
                Hook::ServerStarted => assert_eq!(site.this, None),
                Hook::TickServerHead => *canceled = true,
                Hook::GetDestroyProgressReturn => {
                    if let Slot::Float(f) = value {
                        *value = Slot::Float(*f * 2.0);
                    }
                }
                _ => *value = Slot::Bool(true),
            }
        })));
        assert!(!install(Box::new(|_, _, _, _, _| {})));
        run(Hook::TickServerHead, &mut c, &mut NoHost, site);
        assert_eq!((CALLS.load(Ordering::Relaxed), c.is_canceled()), (0, false));

        attach(Hook::TickServerHead);
        run(Hook::TickServerHead, &mut c, &mut NoHost, site);
        assert!(c.is_canceled());
        attach(Hook::GetDestroyProgressReturn);
        let mut p = Call::returning(0.25f32);
        run(Hook::GetDestroyProgressReturn, &mut p, &mut NoHost, |_| {
            Site::default()
        });
        assert_eq!(p.into_value(), Some(0.5));
        attach(Hook::GetDropsReturn);
        let mut drops = Call::returning(vec![1, 2]);
        run(Hook::GetDropsReturn, &mut drops, &mut NoHost, |_| {
            Site::default()
        });
        assert_eq!(drops.into_value(), Some(vec![1, 2]));
        event(Hook::ServerStarted, &mut NoHost, |_| Site::default());
        assert_eq!(CALLS.load(Ordering::Relaxed), 3);
        attach(Hook::ServerStarted);
        event(Hook::ServerStarted, &mut NoHost, |_| Site::default());
        assert_eq!(CALLS.load(Ordering::Relaxed), 4);

        detach(Hook::TickServerHead);
        detach(Hook::TickServerHead);
        assert_eq!(attached(Hook::TickServerHead), 0);
        let mut c = Call::<()>::head();
        run(
            Hook::TickServerHead,
            &mut c,
            &mut NoHost,
            |_| unreachable!(),
        );
        assert!(!c.is_canceled());
    }
}
