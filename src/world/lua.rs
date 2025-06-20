use mlua::{FromLua, Lua, LuaSerdeExt, UserData, UserDataFields, UserDataMethods, Value};

use crate::{
    common::{
        ObjectId, ObjectTypeId, Position, timestamp_secs, workdefinitions::RemakeMode,
        write_quantized_rotation,
    },
    ipc::zone::{
        ActionEffect, ActorControlCategory, ActorControlSelf, DamageElement, DamageKind,
        DamageType, EffectKind, EventScene, ServerZoneIpcData, ServerZoneIpcSegment, Warp,
    },
    opcodes::ServerZoneIpcType,
    packet::{PacketSegment, SegmentData, SegmentType},
};

use super::{PlayerData, StatusEffects, Zone, connection::TeleportQuery};

pub enum Task {
    ChangeTerritory { zone_id: u16 },
    SetRemakeMode(RemakeMode),
    Warp { warp_id: u32 },
    BeginLogOut,
    FinishEvent { handler_id: u32 },
    SetClassJob { classjob_id: u8 },
    WarpAetheryte { aetheryte_id: u32 },
}

#[derive(Default)]
pub struct LuaPlayer {
    pub player_data: PlayerData,
    pub status_effects: StatusEffects,
    pub queued_segments: Vec<PacketSegment<ServerZoneIpcSegment>>,
    pub queued_tasks: Vec<Task>,
}

impl LuaPlayer {
    fn queue_segment(&mut self, segment: PacketSegment<ServerZoneIpcSegment>) {
        self.queued_segments.push(segment);
    }

    fn create_segment_target(
        &mut self,
        op_code: ServerZoneIpcType,
        data: ServerZoneIpcData,
        source_actor: u32,
        target_actor: u32,
    ) {
        let ipc = ServerZoneIpcSegment {
            op_code,
            timestamp: timestamp_secs(),
            data,
            ..Default::default()
        };

        self.queue_segment(PacketSegment {
            source_actor,
            target_actor,
            segment_type: SegmentType::Ipc,
            data: SegmentData::Ipc { data: ipc },
        });
    }

    fn create_segment_self(&mut self, op_code: ServerZoneIpcType, data: ServerZoneIpcData) {
        self.create_segment_target(
            op_code,
            data,
            self.player_data.actor_id,
            self.player_data.actor_id,
        );
    }

    fn send_message(&mut self, message: &str) {
        let op_code = ServerZoneIpcType::ServerChatMessage;
        let data = ServerZoneIpcData::ServerChatMessage {
            message: message.to_string(),
            unk: 0,
        };

        self.create_segment_self(op_code, data);
    }

    fn give_status_effect(&mut self, effect_id: u16, duration: f32) {
        self.status_effects.add(effect_id, duration);
    }

    fn play_scene(
        &mut self,
        target: ObjectTypeId,
        event_id: u32,
        scene: u16,
        scene_flags: u32,
        param: u8,
    ) {
        let op_code = ServerZoneIpcType::EventScene;
        let data = ServerZoneIpcData::EventScene(EventScene {
            actor_id: target,
            event_id,
            scene,
            scene_flags,
            unk2: param,
            ..Default::default()
        });

        self.create_segment_self(op_code, data);
    }

    fn set_position(&mut self, position: Position, rotation: f32) {
        let op_code = ServerZoneIpcType::Warp;
        let data = ServerZoneIpcData::Warp(Warp {
            dir: write_quantized_rotation(&rotation),
            position,
            ..Default::default()
        });

        self.create_segment_self(op_code, data);
    }

    fn set_festival(&mut self, festival1: u32, festival2: u32, festival3: u32, festival4: u32) {
        let op_code = ServerZoneIpcType::ActorControlSelf;
        let data = ServerZoneIpcData::ActorControlSelf(ActorControlSelf {
            category: ActorControlCategory::SetFestival {
                festival1,
                festival2,
                festival3,
                festival4,
            },
        });

        self.create_segment_self(op_code, data);
    }

    fn change_territory(&mut self, zone_id: u16) {
        self.queued_tasks.push(Task::ChangeTerritory { zone_id });
    }

    fn set_remake_mode(&mut self, mode: RemakeMode) {
        self.queued_tasks.push(Task::SetRemakeMode(mode));
    }

    fn warp(&mut self, warp_id: u32) {
        self.queued_tasks.push(Task::Warp { warp_id });
    }

    fn begin_log_out(&mut self) {
        self.queued_tasks.push(Task::BeginLogOut);
    }

    fn finish_event(&mut self, handler_id: u32) {
        self.queued_tasks.push(Task::FinishEvent { handler_id });
    }

    fn set_classjob(&mut self, classjob_id: u8) {
        self.queued_tasks.push(Task::SetClassJob { classjob_id });
    }

    fn warp_aetheryte(&mut self, aetheryte_id: u32) {
        self.queued_tasks.push(Task::WarpAetheryte { aetheryte_id });
    }
}

impl UserData for LuaPlayer {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("send_message", |_, this, message: String| {
            this.send_message(&message);
            Ok(())
        });
        methods.add_method_mut(
            "give_status_effect",
            |_, this, (effect_id, duration): (u16, f32)| {
                this.give_status_effect(effect_id, duration);
                Ok(())
            },
        );
        methods.add_method_mut(
            "play_scene",
            |_, this, (target, event_id, scene, scene_flags, param): (ObjectTypeId, u32, u16, u32, u8)| {
                this.play_scene(target, event_id, scene, scene_flags, param);
                Ok(())
            },
        );
        methods.add_method_mut(
            "set_position",
            |lua, this, (position, rotation): (Value, Value)| {
                let position: Position = lua.from_value(position).unwrap();
                let rotation: f32 = lua.from_value(rotation).unwrap();
                this.set_position(position, rotation);
                Ok(())
            },
        );
        methods.add_method_mut(
            "set_festival",
            |_, this, (festival1, festival2, festival3, festival4): (u32, u32, u32, u32)| {
                this.set_festival(festival1, festival2, festival3, festival4);
                Ok(())
            },
        );
        methods.add_method_mut("change_territory", |_, this, zone_id: u16| {
            this.change_territory(zone_id);
            Ok(())
        });
        methods.add_method_mut("set_remake_mode", |lua, this, mode: Value| {
            let mode: RemakeMode = lua.from_value(mode).unwrap();
            this.set_remake_mode(mode);
            Ok(())
        });
        methods.add_method_mut("warp", |_, this, warp_id: u32| {
            this.warp(warp_id);
            Ok(())
        });
        methods.add_method_mut("begin_log_out", |_, this, _: ()| {
            this.begin_log_out();
            Ok(())
        });
        methods.add_method_mut("finish_event", |_, this, handler_id: u32| {
            this.finish_event(handler_id);
            Ok(())
        });
        methods.add_method_mut("set_classjob", |_, this, classjob_id: u8| {
            this.set_classjob(classjob_id);
            Ok(())
        });
        methods.add_method_mut("warp_aetheryte", |_, this, aetheryte_id: u32| {
            this.warp_aetheryte(aetheryte_id);
            Ok(())
        });
    }

    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("id", |_, this| {
            Ok(ObjectTypeId {
                object_id: ObjectId(this.player_data.actor_id),
                object_type: 0,
            })
        });

        fields.add_field_method_get("teleport_query", |_, this| {
            Ok(this.player_data.teleport_query.clone())
        });
        fields.add_field_method_get("rotation", |_, this| Ok(this.player_data.rotation));
        fields.add_field_method_get("position", |_, this| Ok(this.player_data.position));
    }
}

impl UserData for TeleportQuery {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("aetheryte_id", |_, this| Ok(this.aetheryte_id));
    }
}

impl UserData for Position {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("x", |_, this| Ok(this.x));
        fields.add_field_method_get("y", |_, this| Ok(this.y));
        fields.add_field_method_get("z", |_, this| Ok(this.z));
    }
}

impl UserData for ObjectTypeId {}

impl FromLua for ObjectTypeId {
    fn from_lua(value: Value, _: &Lua) -> mlua::Result<Self> {
        match value {
            Value::UserData(ud) => Ok(*ud.borrow::<Self>()?),
            _ => unreachable!(),
        }
    }
}

impl UserData for Zone {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "get_pop_range",
            |lua: &Lua, this, id: u32| -> mlua::Result<mlua::Value> {
                if let Some(pop_range) = this.find_pop_range(id) {
                    let trans = pop_range.0.transform.translation;
                    return lua.pack(Position {
                        x: trans[0],
                        y: trans[1],
                        z: trans[2],
                    });
                } else {
                    tracing::warn!("Failed to find pop range for {id}!");
                }
                Ok(mlua::Nil)
            },
        );
    }
}

#[derive(Clone, Debug, Default)]
pub struct EffectsBuilder {
    pub effects: Vec<ActionEffect>,
}

impl UserData for EffectsBuilder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("damage", |lua, this, (damage_kind, damage_type, damage_element, amount): (Value, Value, Value, u16)| {
            let damage_kind: DamageKind = lua.from_value(damage_kind).unwrap();
            let damage_type: DamageType = lua.from_value(damage_type).unwrap();
            let damage_element: DamageElement = lua.from_value(damage_element).unwrap();

            this.effects.push(ActionEffect {
                kind: EffectKind::Damage {
                    damage_kind,
                    damage_type,
                    damage_element,
                    bonus_percent: 0,
                    unk3: 0,
                    unk4: 0,
                    amount,
                },
                ..Default::default()
            });
            Ok(())
        });
    }
}

impl FromLua for EffectsBuilder {
    fn from_lua(value: Value, _: &Lua) -> mlua::Result<Self> {
        match value {
            Value::UserData(ud) => Ok(ud.borrow::<Self>()?.clone()),
            _ => unreachable!(),
        }
    }
}
