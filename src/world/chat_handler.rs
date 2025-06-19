use crate::{
    inventory::Storage,
    ipc::zone::{ActorControlCategory, ActorControlSelf, ChatMessage, GameMasterRank},
    world::ToServer,
};

use super::ZoneConnection;

pub struct ChatHandler {}

impl ChatHandler {
    pub async fn handle_chat_message(connection: &mut ZoneConnection, chat_message: &ChatMessage) {
        if connection.player_data.gm_rank == GameMasterRank::NormalUser {
            tracing::info!("Rejecting debug command because the user is not GM!");
            return;
        }

        let parts: Vec<&str> = chat_message.message.split(' ').collect();
        match parts[0] {
            "!spawnnpc" => {
                connection
                    .handle
                    .send(ToServer::DebugNewNpc(
                        connection.id,
                        connection.player_data.actor_id,
                    ))
                    .await;
            }
            "!spawnmonster" => {
                connection
                    .handle
                    .send(ToServer::DebugNewEnemy(
                        connection.id,
                        connection.player_data.actor_id,
                    ))
                    .await;
            }
            "!spawnclone" => {
                connection
                    .handle
                    .send(ToServer::DebugSpawnClone(
                        connection.id,
                        connection.player_data.actor_id,
                    ))
                    .await;
            }
            "!unlockaction" => {
                let parts: Vec<&str> = chat_message.message.split(' ').collect();
                let id = parts[1].parse::<u32>().unwrap();

                connection
                    .actor_control_self(ActorControlSelf {
                        category: ActorControlCategory::ToggleActionUnlock { id, unlocked: true },
                    })
                    .await;
            }
            "!equip" => {
                let (_, name) = chat_message.message.split_once(' ').unwrap();

                {
                    let mut gamedata = connection.gamedata.lock().unwrap();

                    if let Some((equip_category, id)) = gamedata.get_item_by_name(name) {
                        let slot = gamedata.get_equipslot_category(equip_category).unwrap();

                        connection
                            .player_data
                            .inventory
                            .equipped
                            .get_slot_mut(slot as u16)
                            .id = id;
                        connection
                            .player_data
                            .inventory
                            .equipped
                            .get_slot_mut(slot as u16)
                            .quantity = 1;
                    }
                }

                connection.send_inventory(true).await;
            }
            "!festival" => {
                // this only sets the festival for the player that called this command
                let msg_parts: Vec<&str> = chat_message.message.split(' ').collect();
                let mut festival1: u32 = 0;
                let mut festival2: u32 = 0;
                let mut festival3: u32 = 0;
                let mut festival4: u32 = 0;

                if msg_parts.len() > 1 {
                    festival1 = msg_parts[1].parse::<u32>().unwrap();
                } 
                if msg_parts.len() > 2 {
                    festival2 = msg_parts[2].parse::<u32>().unwrap();
                }
                if msg_parts.len() > 3 {
                    festival3 = msg_parts[3].parse::<u32>().unwrap();
                }
                if msg_parts.len() > 4 {
                    festival4 = msg_parts[4].parse::<u32>().unwrap();
                }

                connection.actor_control_self(ActorControlSelf {
                    category: ActorControlCategory::SetFestival { festival1, festival2, festival3, festival4 }
                }).await;

            }
            _ => {}
        }
    }
}
