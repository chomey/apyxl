use crate::service::social;
use std::collections::HashMap;

#[derive(Default)]
pub struct User {
    id: Id,
    display: Display,

    // feature: maps
    // feature: nested type dependency - generator will import social.rs
    friends: HashMap<Id, social::Friend>,

    // feature: complex nested types
    equipment_slots: HashMap<String, Option<Vec<Item>>>,

    // feature: user type in parser config
    special_id: SpecialId,
}

pub enum Presence {
    Offline,
    Online,
    Invalid = 999,
}

#[derive(Default)]
pub struct Display {
    display_name: String,
    discriminator: String,
    presence: Presence,
    // feature: optionals
    motd: Option<String>,
}

#[derive(Default)]
pub struct Item {
    id: String,
    data: String,
}

// pub type Id = u128; todo aliases?
#[derive(Default)]
pub struct Id {
    value: u128,
}

// todo methods on dto types
// impl User {
//
// }
