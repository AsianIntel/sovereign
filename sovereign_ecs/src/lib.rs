use hecs::{
    Component, DynamicBundle, NoSuchEntity, Query, QueryBorrow, QueryOne, World as HecsWorld,
};
use std::{any::TypeId, collections::HashMap};

pub use hecs::{CommandBuffer, Entity, EntityBuilder, PreparedQuery};

pub struct World {
    world: HecsWorld,
    singletons: HashMap<TypeId, Entity>,
}

pub struct ParentOf(pub Entity);

impl World {
    pub fn new() -> Self {
        Self {
            world: HecsWorld::new(),
            singletons: HashMap::new(),
        }
    }

    pub fn get(&self) -> &HecsWorld {
        &self.world
    }

    pub fn get_mut(&mut self) -> &mut HecsWorld {
        &mut self.world
    }

    pub fn set_singleton<T: Send + Sync + 'static>(&mut self, singleton: T) {
        let entity = self.world.spawn((singleton,));
        self.singletons.insert(TypeId::of::<T>(), entity);
    }

    pub fn get_singleton<T: Send + Sync + 'static>(&self) -> QueryOne<'_, (&mut T,)> {
        let entity = self.singletons.get(&TypeId::of::<T>()).unwrap();
        let query = self.world.query_one(*entity).unwrap();
        query
    }

    pub fn get_singleton_mut<T: Send + Sync + 'static>(&mut self) -> <&T as Query>::Item<'_> {
        let entity = self.singletons.get(&TypeId::of::<T>()).unwrap();
        let query = self.world.query_one_mut::<(&T,)>(*entity).unwrap();
        query.0
    }

    pub fn query<Q: Query>(&self) -> QueryBorrow<'_, Q> {
        self.world.query::<Q>()
    }

    pub fn insert_one(
        &mut self,
        entity: Entity,
        component: impl Component,
    ) -> Result<(), NoSuchEntity> {
        self.world.insert_one(entity, component)
    }

    pub fn spawn(&mut self, components: impl DynamicBundle) -> Entity {
        self.world.spawn(components)
    }
}
