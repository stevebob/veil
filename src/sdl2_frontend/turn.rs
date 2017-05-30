use std::result;
use std::collections::HashMap;
use rand::Rng;
use sdl2_frontend::renderer::GameRenderer;
use sdl2_frontend::player_render;
use sdl2_frontend::player_turn;
use sdl2::EventPump;
use knowledge::PlayerKnowledgeGrid;
use reaction::Reaction;
use behaviour::*;
use entity_store::*;
use spatial_hash::*;
use entity_id_allocator::*;
use content::ActionType;
use observation::shadowcast::ShadowcastEnv;
use meta_action::*;
use policy::*;
use entity_observe;

#[derive(Debug)]
pub enum Error {
    MissingNpcKnowledge,
    MissingNpcBehaviour,
    PlayerTurnError,
    NpcTurnError,
}
pub type Result<T> = result::Result<T, Error>;

pub enum TurnResolution {
    Reschedule,
    External(External),
}

pub struct TurnEnv<'a, 'renderer: 'a, R: 'a + Rng> {
    pub renderer: &'a mut GameRenderer<'renderer>,
    pub input: &'a mut EventPump,
    pub reactions: &'a mut Vec<Reaction>,
    pub change: &'a mut EntityStoreChange,
    pub entity_store: &'a mut EntityStore,
    pub id_allocator: &'a mut EntityIdAllocator,
    pub spatial_hash: &'a mut SpatialHashTable,
    pub behaviour_env: &'a mut BehaviourEnv,
    pub player_id: EntityId,
    pub entity_id: EntityId,
    pub player_knowledge: &'a mut PlayerKnowledgeGrid,
    pub knowledge: &'a mut HashMap<EntityId, PlayerKnowledgeGrid>,
    pub behaviour: &'a mut HashMap<EntityId, BehaviourState>,
    pub shadowcast: &'a mut ShadowcastEnv,
    pub time: &'a mut u64,
    pub policy: &'a GamePolicy,
    pub rng: &'a mut R,
}

pub struct ActEnvPlayer<'a, 'renderer: 'a, R: 'a + Rng> {
    pub renderer: &'a mut GameRenderer<'renderer>,
    pub input: &'a mut EventPump,
    pub change: &'a mut EntityStoreChange,
    pub entity_store: &'a mut EntityStore,
    pub spatial_hash: &'a mut SpatialHashTable,
    pub entity_id: EntityId,
    pub knowledge: &'a mut PlayerKnowledgeGrid,
    pub shadowcast: &'a mut ShadowcastEnv,
    pub time: &'a mut u64,
    pub policy: &'a GamePolicy,
    pub rng: &'a mut R,
}

impl<'a, 'renderer: 'a, R: Rng> ActEnvPlayer<'a, 'renderer, R> {
    fn act(mut self) -> player_turn::Result<MetaAction> {
        player_turn::player_turn(&mut self)
    }

    pub fn render(&mut self) -> player_render::Result<()>{
        player_render::player_render(
            self.entity_id,
            self.entity_store,
            self.spatial_hash,
            *self.time,
            self.knowledge,
            self.shadowcast,
            self.renderer
        )
    }
}

pub struct ActEnvNpc<'a> {
    pub entity_store: &'a EntityStore,
    pub spatial_hash: &'a SpatialHashTable,
    pub entity_id: EntityId,
    pub knowledge: &'a mut PlayerKnowledgeGrid,
    pub behaviour_env: &'a mut BehaviourEnv,
    pub behaviour_state: &'a mut BehaviourState,
    pub shadowcast: &'a mut ShadowcastEnv,
    pub time: &'a mut u64,
}

impl<'a> ActEnvNpc<'a> {
    fn act(self) -> entity_observe::Result<ActionType> {

        let metadata = entity_observe::entity_observe(self.entity_id,
                                                      self.entity_store,
                                                      self.spatial_hash,
                                                      *self.time,
                                                      self.knowledge,
                                                      self.shadowcast)?;

        Ok(attack::attack(self.entity_id, self.entity_store, self.knowledge, self.behaviour_env, self.behaviour_state).or_else(|| {
            patrol::patrol(self.entity_id, self.entity_store, self.knowledge, metadata, *self.time, self.behaviour_env, self.behaviour_state)
        }).unwrap_or(ActionType::Null))
    }
}

struct CommitEnv<'a, 'renderer: 'a> {
    pub renderer: &'a mut GameRenderer<'renderer>,
    pub change: &'a mut EntityStoreChange,
    pub entity_store: &'a mut EntityStore,
    pub spatial_hash: &'a mut SpatialHashTable,
    pub player_knowledge: &'a mut PlayerKnowledgeGrid,
    pub shadowcast: &'a mut ShadowcastEnv,
    pub time: &'a mut u64,
    pub reactions: &'a mut Vec<Reaction>,
    pub id_allocator: &'a mut EntityIdAllocator,
    pub policy: &'a GamePolicy,
}

impl<'a, 'renderer: 'a> CommitEnv<'a, 'renderer> {
    fn commit(self, initial_action: ActionType) {
        self.reactions.clear();
        self.reactions.push(Reaction::immediate(initial_action));

        while let Some(reaction) = self.reactions.pop() {
            reaction.action.populate(self.change, self.entity_store);

            if self.policy.on_change(self.change, self.entity_store, self.spatial_hash, self.reactions) {
                *self.time += 1;
                self.spatial_hash.update(self.entity_store, self.change, *self.time);
                self.entity_store.commit_change(self.change);
            } else {
                self.change.clear();
            }
        }
    }
}

impl<'a, 'renderer: 'a, R: Rng> TurnEnv<'a, 'renderer, R> {
    pub fn take_turn(self) -> Result<TurnResolution> {

        let initial_action = if self.entity_store.player.contains(&self.entity_id) {
            let meta_action = ActEnvPlayer {
                renderer: self.renderer,
                input: self.input,
                change: self.change,
                entity_store: self.entity_store,
                spatial_hash: self.spatial_hash,
                entity_id: self.entity_id,
                knowledge: self.player_knowledge,
                shadowcast: self.shadowcast,
                time: self.time,
                policy: self.policy,
                rng: self.rng,
            }.act().map_err(|_| Error::PlayerTurnError)?;

            match meta_action {
                MetaAction::Action(action) => action,
                MetaAction::External(external) => return Ok(TurnResolution::External(external)),
            }
        } else {
            ActEnvNpc {
                entity_store: self.entity_store,
                spatial_hash: self.spatial_hash,
                entity_id: self.entity_id,
                knowledge: self.knowledge.get_mut(&self.entity_id).ok_or(Error::MissingNpcKnowledge)?,
                behaviour_state: self.behaviour.get_mut(&self.entity_id).ok_or(Error::MissingNpcBehaviour)?,
                behaviour_env: self.behaviour_env,
                shadowcast: self.shadowcast,
                time: self.time,
            }.act().map_err(|_| Error::NpcTurnError)?
        };

        CommitEnv {
            renderer: self.renderer,
            change: self.change,
            entity_store: self.entity_store,
            spatial_hash: self.spatial_hash,
            player_knowledge: self.player_knowledge,
            shadowcast: self.shadowcast,
            time: self.time,
            reactions: self.reactions,
            id_allocator: self.id_allocator,
            policy: self.policy,
        }.commit(initial_action);

        Ok(TurnResolution::Reschedule)
    }
}
