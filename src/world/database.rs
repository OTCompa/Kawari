use std::{io::Read, sync::Mutex};

use rusqlite::Connection;
use serde::Deserialize;

use crate::{
    AETHERYTE_UNLOCK_BITMASK_SIZE, COMPLETED_QUEST_BITMASK_SIZE, UNLOCK_BITMASK_SIZE,
    common::{
        CustomizeData, GameData, Position,
        workdefinitions::{CharaMake, ClientSelectData, RemakeMode},
    },
    inventory::{Inventory, Item, Storage},
    ipc::{
        lobby::{CharacterDetails, CharacterFlag},
        zone::GameMasterRank,
    },
};

use super::PlayerData;

pub struct WorldDatabase {
    connection: Mutex<Connection>,
}

pub struct CharacterData {
    pub name: String,
    pub chara_make: CharaMake, // probably not the ideal way to store this?
    pub city_state: u8,
    pub position: Position,
    pub zone_id: u16,
}

impl Default for WorldDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl WorldDatabase {
    pub fn new() -> Self {
        let connection = Connection::open("world.db").expect("Failed to open database!");

        // Create characters table
        {
            let query = "CREATE TABLE IF NOT EXISTS characters (content_id INTEGER PRIMARY KEY, service_account_id INTEGER, actor_id INTEGER);";
            connection.execute(query, ()).unwrap();
        }

        // Create characters data table
        {
            let query = "CREATE TABLE IF NOT EXISTS character_data (content_id INTEGER PRIMARY KEY, name STRING, chara_make STRING, city_state INTEGER, zone_id INTEGER, pos_x REAL, pos_y REAL, pos_z REAL, rotation REAL, inventory STRING, remake_mode INTEGER, gm_rank INTEGER, classjob_id INTEGER, classjob_levels STRING, classjob_exp STRING, unlocks STRING, aetherytes STRING, completed_quests STRING);";
            connection.execute(query, ()).unwrap();
        }

        Self {
            connection: Mutex::new(connection),
        }
    }

    pub fn import_character(&self, game_data: &mut GameData, service_account_id: u32, path: &str) {
        tracing::info!("Importing character backup from {path}...");

        let file = std::fs::File::open(path).unwrap();

        let mut archive = zip::ZipArchive::new(file).unwrap();

        #[derive(Deserialize)]
        struct GenericValue {
            value: i32,
        }

        #[derive(Deserialize)]
        struct NamedayValue {
            day: i32,
            month: i32,
        }

        #[derive(Deserialize)]
        struct ClassJobLevelValue {
            level: i32,
            exp: Option<u32>,
            value: i32,
        }

        #[derive(Deserialize)]
        struct InventoryItem {
            slot: i32,
            quantity: u32,
            condition: u16,
            id: u32,
            glamour_id: u32,
        }

        #[derive(Deserialize)]
        struct InventoryContainer {
            items: Vec<InventoryItem>,
        }

        #[derive(Deserialize)]
        struct CharacterJson {
            name: String,
            city_state: GenericValue,
            nameday: NamedayValue,
            guardian: GenericValue,
            voice: i32,
            classjob_levels: Vec<ClassJobLevelValue>,

            inventory1: InventoryContainer,
            inventory2: InventoryContainer,
            inventory3: InventoryContainer,
            inventory4: InventoryContainer,
            equipped_items: InventoryContainer,

            currency: InventoryContainer,

            armory_off_hand: InventoryContainer,
            armory_head: InventoryContainer,
            armory_body: InventoryContainer,
            armory_hands: InventoryContainer,
            armory_legs: InventoryContainer,
            armory_ear: InventoryContainer,
            armory_neck: InventoryContainer,
            armory_wrist: InventoryContainer,
            armory_rings: InventoryContainer,
            armory_soul_crystal: InventoryContainer,
            armory_main_hand: InventoryContainer,

            unlock_flags: Vec<u8>,
            unlock_aetherytes: Vec<u8>,
        }

        let character: CharacterJson;
        {
            let mut character_file = archive.by_name("character.json").unwrap();

            let mut json_string = String::new();
            character_file.read_to_string(&mut json_string).unwrap();

            character = serde_json::from_str(&json_string).unwrap();
        }

        if !self.check_is_name_free(&character.name) {
            let name = character.name;
            tracing::warn!("* Skipping since {name} already exists.");
            return;
        }

        let charsave_file = archive.by_name("FFXIV_CHARA_01.dat").unwrap();
        let charsave_bytes: Vec<u8> = charsave_file.bytes().map(|x| x.unwrap()).collect();
        let charsave =
            physis::savedata::chardat::CharacterData::from_existing(&charsave_bytes).unwrap();

        let customize = CustomizeData::from(charsave.customize);

        let chara_make = CharaMake {
            customize,
            voice_id: character.voice,
            guardian: character.guardian.value,
            birth_month: character.nameday.month,
            birth_day: character.nameday.day,
            classjob_id: 5,
            unk2: 1,
        };

        let (_, actor_id) = self.create_player_data(
            service_account_id,
            &character.name,
            &chara_make.to_json(),
            character.city_state.value as u8,
            132,
            Inventory::default(),
        );

        let mut player_data = self.find_player_data(actor_id);

        // import jobs
        for classjob in &character.classjob_levels {
            // find the array index of the job
            let index = game_data
                .get_exp_array_index(classjob.value as u16)
                .unwrap();

            player_data.classjob_levels[index as usize] = classjob.level;
            if let Some(exp) = classjob.exp {
                player_data.classjob_exp[index as usize] = exp;
            }
        }

        let process_inventory_container =
            |container: &InventoryContainer, target: &mut dyn Storage| {
                for item in &container.items {
                    if item.slot as u32 > target.max_slots() {
                        continue;
                    }
                    *target.get_slot_mut(item.slot as u16) = Item {
                        quantity: item.quantity,
                        id: item.id,
                        condition: item.condition,
                        glamour_catalog_id: item.glamour_id,
                    };
                }
            };

        // import inventory
        process_inventory_container(&character.inventory1, &mut player_data.inventory.pages[0]);
        process_inventory_container(&character.inventory2, &mut player_data.inventory.pages[1]);
        process_inventory_container(&character.inventory3, &mut player_data.inventory.pages[2]);
        process_inventory_container(&character.inventory4, &mut player_data.inventory.pages[3]);
        process_inventory_container(
            &character.equipped_items,
            &mut player_data.inventory.equipped,
        );

        process_inventory_container(&character.currency, &mut player_data.inventory.currency);

        process_inventory_container(
            &character.armory_off_hand,
            &mut player_data.inventory.armoury_off_hand,
        );
        process_inventory_container(
            &character.armory_head,
            &mut player_data.inventory.armoury_head,
        );
        process_inventory_container(
            &character.armory_body,
            &mut player_data.inventory.armoury_body,
        );
        process_inventory_container(
            &character.armory_hands,
            &mut player_data.inventory.armoury_hands,
        );
        process_inventory_container(
            &character.armory_legs,
            &mut player_data.inventory.armoury_legs,
        );
        process_inventory_container(
            &character.armory_ear,
            &mut player_data.inventory.armoury_earring,
        );
        process_inventory_container(
            &character.armory_neck,
            &mut player_data.inventory.armoury_necklace,
        );
        process_inventory_container(
            &character.armory_wrist,
            &mut player_data.inventory.armoury_bracelet,
        );
        process_inventory_container(
            &character.armory_rings,
            &mut player_data.inventory.armoury_rings,
        );
        process_inventory_container(
            &character.armory_soul_crystal,
            &mut player_data.inventory.armoury_soul_crystal,
        );
        process_inventory_container(
            &character.armory_main_hand,
            &mut player_data.inventory.armoury_main_hand,
        );

        // import unlock flags
        player_data.unlocks = character.unlock_flags;
        player_data.aetherytes = character.unlock_aetherytes;

        self.commit_player_data(&player_data);

        tracing::info!("{} added to the world!", character.name);
    }

    pub fn find_player_data(&self, actor_id: u32) -> PlayerData {
        let connection = self.connection.lock().unwrap();

        let mut stmt = connection
            .prepare("SELECT content_id, service_account_id FROM characters WHERE actor_id = ?1")
            .unwrap();
        let (content_id, account_id) = stmt
            .query_row((actor_id,), |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap();

        stmt = connection
            .prepare("SELECT pos_x, pos_y, pos_z, rotation, zone_id, inventory, gm_rank, classjob_id, classjob_levels, classjob_exp, unlocks, aetherytes, completed_quests FROM character_data WHERE content_id = ?1")
            .unwrap();
        let (
            pos_x,
            pos_y,
            pos_z,
            rotation,
            zone_id,
            inventory_json,
            gm_rank,
            classjob_id,
            classjob_levels,
            classjob_exp,
            unlocks,
            aetherytes,
            completed_quests,
        ): (
            f32,
            f32,
            f32,
            f32,
            u16,
            String,
            u8,
            i32,
            String,
            String,
            String,
            String,
            String,
        ) = stmt
            .query_row((content_id,), |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                ))
            })
            .unwrap();

        let inventory = serde_json::from_str(&inventory_json).unwrap();

        PlayerData {
            actor_id,
            content_id,
            account_id,
            position: Position {
                x: pos_x,
                y: pos_y,
                z: pos_z,
            },
            rotation,
            zone_id,
            inventory,
            gm_rank: GameMasterRank::try_from(gm_rank).unwrap(),
            classjob_id: classjob_id as u8,
            classjob_levels: serde_json::from_str(&classjob_levels).unwrap(),
            classjob_exp: serde_json::from_str(&classjob_exp).unwrap(),
            unlocks: serde_json::from_str(&unlocks).unwrap(),
            aetherytes: serde_json::from_str(&aetherytes).unwrap(),
            completed_quests: serde_json::from_str(&completed_quests).unwrap(),
            ..Default::default()
        }
    }

    /// Commit the dynamic player data back to the database
    pub fn commit_player_data(&self, data: &PlayerData) {
        let connection = self.connection.lock().unwrap();

        let mut stmt = connection
            .prepare("UPDATE character_data SET zone_id=?1, pos_x=?2, pos_y=?3, pos_z=?4, rotation=?5, inventory=?6, classjob_id=?7, classjob_levels=?8, classjob_exp=?9, unlocks=?10, aetherytes=?11, completed_quests=?12 WHERE content_id = ?13")
            .unwrap();
        stmt.execute((
            data.zone_id,
            data.position.x,
            data.position.y,
            data.position.z,
            data.rotation,
            serde_json::to_string(&data.inventory).unwrap(),
            data.classjob_id,
            serde_json::to_string(&data.classjob_levels).unwrap(),
            serde_json::to_string(&data.classjob_exp).unwrap(),
            serde_json::to_string(&data.unlocks).unwrap(),
            serde_json::to_string(&data.aetherytes).unwrap(),
            serde_json::to_string(&data.completed_quests).unwrap(),
            data.content_id,
        ))
        .unwrap();
    }

    // TODO: from/to sql int

    pub fn find_actor_id(&self, content_id: u64) -> u32 {
        let connection = self.connection.lock().unwrap();

        let mut stmt = connection
            .prepare("SELECT actor_id FROM characters WHERE content_id = ?1")
            .unwrap();

        stmt.query_row((content_id,), |row| row.get(0)).unwrap()
    }

    pub fn get_character_list(
        &self,
        service_account_id: u32,
        world_id: u16,
        world_name: &str,
        game_data: &mut GameData,
    ) -> Vec<CharacterDetails> {
        let connection = self.connection.lock().unwrap();

        let content_actor_ids: Vec<(u32, u32)>;

        // find the content ids associated with the service account
        {
            let mut stmt = connection
                .prepare(
                    "SELECT content_id, actor_id FROM characters WHERE service_account_id = ?1",
                )
                .unwrap();

            content_actor_ids = stmt
                .query_map((service_account_id,), |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap()
                .map(|x| x.unwrap())
                .collect();
        }

        let mut characters = Vec::new();

        for (index, (content_id, actor_id)) in content_actor_ids.iter().enumerate() {
            let mut stmt = connection
                .prepare(
                    "SELECT name, chara_make, zone_id, inventory, remake_mode, classjob_id, classjob_levels FROM character_data WHERE content_id = ?1",
                )
                .unwrap();

            let result: Result<(String, String, u16, String, i32, i32, String), rusqlite::Error> =
                stmt.query_row((content_id,), |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                });

            if let Ok((
                name,
                chara_make,
                zone_id,
                inventory_json,
                remake_mode,
                classjob_id,
                classjob_levels,
            )) = result
            {
                let chara_make = CharaMake::from_json(&chara_make);

                let inventory: Inventory = serde_json::from_str(&inventory_json).unwrap();

                let select_data = ClientSelectData {
                    character_name: name.clone(),
                    current_class: classjob_id,
                    class_levels: serde_json::from_str(&classjob_levels).unwrap(),
                    race: chara_make.customize.race as i32,
                    subrace: chara_make.customize.subrace as i32,
                    gender: chara_make.customize.gender as i32,
                    birth_month: chara_make.birth_month,
                    birth_day: chara_make.birth_day,
                    guardian: chara_make.guardian,
                    unk8: 0,
                    unk9: 0,
                    zone_id: zone_id as i32,
                    content_finder_condition: 0,
                    customize: chara_make.customize,
                    model_main_weapon: inventory.get_main_weapon_id(game_data),
                    model_sub_weapon: 0,
                    model_ids: inventory.get_model_ids(game_data),
                    equip_stain: [0; 10],
                    glasses: [0; 2],
                    remake_mode: RemakeMode::try_from(remake_mode).unwrap(),
                    remake_minutes_remaining: 0,
                    voice_id: chara_make.voice_id,
                    unk20: 0,
                    unk21: 0,
                    world_name: String::new(),
                    unk22: 0,
                    unk23: 0,
                };

                characters.push(CharacterDetails {
                    actor_id: *actor_id,
                    content_id: *content_id as u64,
                    index: index as u8,
                    flags: CharacterFlag::NONE,
                    unk1: [255; 6],
                    origin_server_id: world_id,
                    current_server_id: world_id,
                    character_name: name.clone(),
                    origin_server_name: world_name.to_string(),
                    current_server_name: world_name.to_string(),
                    character_detail_json: select_data.to_json(),
                    unk2: [255; 16],
                    unk3: [4; 5],
                });
            }
        }

        characters
    }

    fn generate_content_id() -> u32 {
        fastrand::u32(..)
    }

    fn generate_actor_id() -> u32 {
        fastrand::u32(..)
    }

    /// Gives (content_id, actor_id)
    pub fn create_player_data(
        &self,
        service_account_id: u32,
        name: &str,
        chara_make_str: &str,
        city_state: u8,
        zone_id: u16,
        inventory: Inventory,
    ) -> (u64, u32) {
        let content_id = Self::generate_content_id();
        let actor_id = Self::generate_actor_id();

        let connection = self.connection.lock().unwrap();

        // fill out the initial classjob
        let chara_make = CharaMake::from_json(chara_make_str);
        let mut classjob_levels = [0i32; 32];
        classjob_levels[chara_make.classjob_id as usize] = 1; // inital level

        let classjob_exp = [0u32; 32];

        // fill out initial unlocks
        let unlocks = vec![0u8; UNLOCK_BITMASK_SIZE];

        // fill out initial aetherytes
        let aetherytes = vec![0u8; AETHERYTE_UNLOCK_BITMASK_SIZE];

        // fill out initial completed quests`
        let completed_quests = vec![0u8; COMPLETED_QUEST_BITMASK_SIZE];

        // insert ids
        connection
            .execute(
                "INSERT INTO characters VALUES (?1, ?2, ?3);",
                (content_id, service_account_id, actor_id),
            )
            .unwrap();

        // insert char data
        connection
            .execute(
                "INSERT INTO character_data VALUES (?1, ?2, ?3, ?4, ?5, 0.0, 0.0, 0.0, 0.0, ?6, 0, 90, ?7, ?8, ?9, ?10, ?11, ?12);",
                (
                    content_id,
                    name,
                    chara_make_str,
                    city_state,
                    zone_id,
                    serde_json::to_string(&inventory).unwrap(),
                    chara_make.classjob_id,
                    serde_json::to_string(&classjob_levels).unwrap(),
                    serde_json::to_string(&classjob_exp).unwrap(),
                    serde_json::to_string(&unlocks).unwrap(),
                    serde_json::to_string(&aetherytes).unwrap(),
                    serde_json::to_string(&completed_quests).unwrap(),
                ),
            )
            .unwrap();

        (content_id as u64, actor_id)
    }

    /// Checks if `name` is in the character data table
    pub fn check_is_name_free(&self, name: &str) -> bool {
        let connection = self.connection.lock().unwrap();

        let mut stmt = connection
            .prepare("SELECT content_id FROM character_data WHERE name = ?1")
            .unwrap();

        !stmt.exists((name,)).unwrap()
    }

    pub fn find_chara_make(&self, content_id: u64) -> CharacterData {
        let connection = self.connection.lock().unwrap();

        let mut stmt = connection
            .prepare(
                "SELECT name, chara_make, city_state, zone_id, pos_x, pos_y, pos_z FROM character_data WHERE content_id = ?1",
            )
            .unwrap();
        let (name, chara_make_json, city_state, zone_id, pos_x, pos_y, pos_z): (
            String,
            String,
            u8,
            u16,
            f32,
            f32,
            f32,
        ) = stmt
            .query_row((content_id,), |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            })
            .unwrap();

        CharacterData {
            name,
            chara_make: CharaMake::from_json(&chara_make_json),
            city_state,
            zone_id,
            position: Position {
                x: pos_x,
                y: pos_y,
                z: pos_z,
            },
        }
    }

    /// Deletes a character and all associated data
    pub fn delete_character(&self, content_id: u64) {
        let connection = self.connection.lock().unwrap();

        // delete data
        {
            let mut stmt = connection
                .prepare("DELETE FROM character_data WHERE content_id = ?1")
                .unwrap();
            stmt.execute((content_id,)).unwrap();
        }

        // delete char
        {
            let mut stmt = connection
                .prepare("DELETE FROM characters WHERE content_id = ?1")
                .unwrap();
            stmt.execute((content_id,)).unwrap();
        }
    }

    /// Sets the remake mode for a character
    pub fn set_remake_mode(&self, content_id: u64, mode: RemakeMode) {
        let connection = self.connection.lock().unwrap();

        let mut stmt = connection
            .prepare("UPDATE character_data SET remake_mode=?1 WHERE content_id = ?2")
            .unwrap();
        stmt.execute((mode as i32, content_id)).unwrap();
    }

    /// Sets the chara make JSON for a character
    pub fn set_chara_make(&self, content_id: u64, chara_make_json: &str) {
        let connection = self.connection.lock().unwrap();

        let mut stmt = connection
            .prepare("UPDATE character_data SET chara_make=?1 WHERE content_id = ?2")
            .unwrap();
        stmt.execute((chara_make_json, content_id)).unwrap();
    }
}
