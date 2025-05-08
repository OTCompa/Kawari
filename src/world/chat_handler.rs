use crate::{
    common::{CustomizeData, ObjectId, ObjectTypeId, timestamp_secs},
    ipc::zone::{
        ActorControl, ActorControlCategory, BattleNpcSubKind, ChatMessage, CommonSpawn, EventStart,
        NpcSpawn, ObjectKind, OnlineStatus, ServerZoneIpcData, ServerZoneIpcSegment,
    },
    opcodes::ServerZoneIpcType,
    packet::{PacketSegment, SegmentData, SegmentType},
    world::{Actor, Event},
};

use super::{LuaPlayer, ZoneConnection};

pub const CUSTOMIZE_DATA: CustomizeData = CustomizeData {
    race: 4,
    gender: 1,
    age: 1,
    height: 50,
    subrace: 7,
    face: 3,
    hair: 5,
    enable_highlights: 0,
    skin_tone: 10,
    right_eye_color: 75,
    hair_tone: 50,
    highlights: 0,
    facial_features: 1,
    facial_feature_color: 19,
    eyebrows: 1,
    left_eye_color: 75,
    eyes: 1,
    nose: 0,
    jaw: 1,
    mouth: 1,
    lips_tone_fur_pattern: 169,
    race_feature_size: 100,
    race_feature_type: 1,
    bust: 100,
    face_paint: 0,
    face_paint_color: 167,
};

pub struct ChatHandler {}

impl ChatHandler {
    pub async fn handle_chat_message(
        connection: &mut ZoneConnection,
        lua_player: &mut LuaPlayer,
        chat_message: &ChatMessage,
    ) {
        tracing::info!("Client sent chat message: {}!", chat_message.message);

        let parts: Vec<&str> = chat_message.message.split(' ').collect();
        match parts[0] {
            "!spawnnpc" => {
                let ipc = ServerZoneIpcSegment {
                    op_code: ServerZoneIpcType::NpcSpawn,
                    timestamp: timestamp_secs(),
                    data: ServerZoneIpcData::NpcSpawn(NpcSpawn {
                        common: CommonSpawn {
                            hp_curr: 100,
                            hp_max: 100,
                            mp_curr: 100,
                            mp_max: 100,
                            look: CUSTOMIZE_DATA,
                            spawn_index: connection.get_free_spawn_index(),
                            bnpc_base: 13498,
                            bnpc_name: 10261,
                            object_kind: ObjectKind::BattleNpc(BattleNpcSubKind::Enemy),
                            target_id: ObjectTypeId {
                                object_id: ObjectId(connection.player_data.actor_id),
                                object_type: 0,
                            }, // target the player
                            level: 1,
                            models: [
                                0,  // head
                                89, // body
                                89, // hands
                                89, // legs
                                89, // feet
                                0,  // ears
                                0,  // neck
                                0,  // wrists
                                0,  // left finger
                                0,  // right finger
                            ],
                            pos: connection.player_data.position,
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                    ..Default::default()
                };

                connection
                    .send_segment(PacketSegment {
                        source_actor: 0x106ad804,
                        target_actor: connection.player_data.actor_id,
                        segment_type: SegmentType::Ipc,
                        data: SegmentData::Ipc { data: ipc },
                    })
                    .await;
            }
            "!spawnmonster" => {
                let spawn_index = connection.get_free_spawn_index();
                
                let bnpc_id: u32;
                let model_id: u16;
                if parts.len() > 2 {
                    bnpc_id = parts[1].parse::<u32>().unwrap();
                    model_id = parts[2].parse::<u16>().unwrap();
                } else {
                    bnpc_id = 13498;
                    model_id = 297;
                }

                // spawn a tiny mandragora
                {
                    let ipc = ServerZoneIpcSegment {
                        op_code: ServerZoneIpcType::NpcSpawn,
                        timestamp: timestamp_secs(),
                        data: ServerZoneIpcData::NpcSpawn(NpcSpawn {
                            aggression_mode: 1,
                            common: CommonSpawn {
                                hp_curr: 91,
                                hp_max: 91,
                                mp_curr: 100,
                                mp_max: 100,
                                spawn_index,
                                bnpc_base: bnpc_id, // TODO: changing this prevents it from spawning...
                                bnpc_name: 405,
                                object_kind: ObjectKind::BattleNpc(BattleNpcSubKind::Enemy),
                                level: 1,
                                battalion: 4,
                                model_chara: model_id,
                                pos: connection.player_data.position,
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                        ..Default::default()
                    };

                    connection
                        .send_segment(PacketSegment {
                            source_actor: 0x106ad804,
                            target_actor: connection.player_data.actor_id,
                            segment_type: SegmentType::Ipc,
                            data: SegmentData::Ipc { data: ipc },
                        })
                        .await;
                }

                connection.actors.push(Actor {
                    id: ObjectId(0x106ad804),
                    hp: 91,
                    spawn_index: spawn_index as u32,
                });
            }
            "!playscene" => {
                let parts: Vec<&str> = chat_message.message.split(' ').collect();
                let event_id = parts[1].parse::<u32>().unwrap();

                // Load the game script for this event on the client
                {
                    let ipc = ServerZoneIpcSegment {
                        op_code: ServerZoneIpcType::EventStart,
                        timestamp: timestamp_secs(),
                        data: ServerZoneIpcData::EventStart(EventStart {
                            target_id: ObjectTypeId {
                                object_id: ObjectId(connection.player_data.actor_id),
                                object_type: 0,
                            },
                            event_type: 15,
                            event_id,
                            flags: 0,
                            event_arg: 182, // zone?
                        }),
                        ..Default::default()
                    };

                    connection
                        .send_segment(PacketSegment {
                            source_actor: connection.player_data.actor_id,
                            target_actor: connection.player_data.actor_id,
                            segment_type: SegmentType::Ipc,
                            data: SegmentData::Ipc { data: ipc },
                        })
                        .await;
                }

                // set our status icon to viewing cutscene
                {
                    let ipc = ServerZoneIpcSegment {
                        op_code: ServerZoneIpcType::ActorControl,
                        timestamp: timestamp_secs(),
                        data: ServerZoneIpcData::ActorControl(ActorControl {
                            category: ActorControlCategory::SetStatusIcon {
                                icon: OnlineStatus::ViewingCutscene,
                            },
                        }),
                        ..Default::default()
                    };

                    connection
                        .send_segment(PacketSegment {
                            source_actor: connection.player_data.actor_id,
                            target_actor: connection.player_data.actor_id,
                            segment_type: SegmentType::Ipc,
                            data: SegmentData::Ipc { data: ipc },
                        })
                        .await;
                }

                let event = match event_id {
                    1245185 => Event::new(1245185, "opening/OpeningLimsaLominsa.lua"),
                    1245186 => Event::new(1245186, "opening/OpeningGridania.lua"),
                    1245187 => Event::new(1245187, "opening/OpeningUldah.lua"),
                    _ => panic!("Unsupported event!"),
                };

                connection.event = Some(event);
                connection
                    .event
                    .as_mut()
                    .unwrap()
                    .enter_territory(lua_player, connection.zone.as_ref().unwrap());
            }
            "!spawnclone" => {
                // spawn another one of us
                let player = &connection.player_data;

                let mut common = connection
                    .get_player_common_spawn(Some(player.position), Some(player.rotation));
                common.spawn_index = connection.get_free_spawn_index();

                let ipc = ServerZoneIpcSegment {
                    op_code: ServerZoneIpcType::NpcSpawn,
                    timestamp: timestamp_secs(),
                    data: ServerZoneIpcData::NpcSpawn(NpcSpawn {
                        common,
                        ..Default::default()
                    }),
                    ..Default::default()
                };

                connection
                    .send_segment(PacketSegment {
                        source_actor: 0x106ad804,
                        target_actor: connection.player_data.actor_id,
                        segment_type: SegmentType::Ipc,
                        data: SegmentData::Ipc { data: ipc },
                    })
                    .await;
            }
            _ => {}
        }
    }
}
