use super::*;

fn setup_world_with_4_channels() -> (World, [Id; 4]) {
    let mut world = setup_world();
    let ids: [Id; 4] = std::array::from_fn(|_| Id::new());
    let names = ["A", "B", "C", "D"];
    for (i, (id, name)) in ids.iter().zip(names.iter()).enumerate() {
        let snapshot = ChannelSnapshot {
            name: Name::new(*name),
            id: *id,
            ..Default::default()
        };
        AddChannelCommand::new(i, snapshot).execute(&mut world);
    }
    (world, ids)
}

fn get_channel_ids(world: &mut World) -> Vec<Id> {
    let order = get_channel_order(world);
    order
        .iter()
        .map(|e| *world.get::<Id>(*e).unwrap())
        .collect()
}

#[test]
fn add_channel() {
    let mut world = setup_world();
    let snapshot = ChannelSnapshot::default();
    let id = snapshot.id;

    AddChannelCommand::new(0, snapshot).execute(&mut world);

    let order = get_channel_order(&mut world);
    assert_eq!(order.len(), 1);

    let entity = id.find_entity(&mut world).unwrap();
    assert_eq!(order[0], entity);
    assert_eq!(
        world.get::<Name>(entity).unwrap().as_str(),
        "unnamed channel"
    );
    assert_eq!(world.get::<ChannelState>(entity).unwrap().gain_value, 1.0);
    assert_eq!(*world.get::<Id>(entity).unwrap(), id);
}

#[test]
fn add_channel_returns_delete_undo() {
    let mut world = setup_world();
    let snapshot = ChannelSnapshot::default();

    let undo = AddChannelCommand::new(0, snapshot)
        .execute(&mut world)
        .unwrap();
    undo.execute(&mut world);

    let order = get_channel_order(&mut world);
    assert_eq!(order.len(), 0);
}

#[test]
fn delete_channel() {
    let mut world = setup_world();
    let snapshot = ChannelSnapshot::default();
    let id = snapshot.id;

    AddChannelCommand::new(0, snapshot).execute(&mut world);

    DeleteChannelCommand::new(id, 0).execute(&mut world);

    let order = get_channel_order(&mut world);
    assert_eq!(order.len(), 0);
    assert!(id.find_entity(&mut world).is_none());
}

#[test]
fn delete_channel_returns_add_undo() {
    let mut world = setup_world();
    let mut snapshot = ChannelSnapshot::default();
    snapshot.name = Name::new("my channel");
    snapshot.state.gain_value = 0.5;
    let id = snapshot.id;

    AddChannelCommand::new(0, snapshot).execute(&mut world);

    let undo = DeleteChannelCommand::new(id, 0)
        .execute(&mut world)
        .unwrap();
    undo.execute(&mut world);

    let order = get_channel_order(&mut world);
    assert_eq!(order.len(), 1);

    let entity = id.find_entity(&mut world).unwrap();
    assert_eq!(world.get::<Name>(entity).unwrap().as_str(), "my channel");
    assert_eq!(world.get::<ChannelState>(entity).unwrap().gain_value, 0.5);
    assert_eq!(*world.get::<Id>(entity).unwrap(), id);
}

#[test]
fn add_delete_roundtrip() {
    let mut world = setup_world();
    let snapshot = ChannelSnapshot::default();
    let id = snapshot.id;

    // Add
    let delete_cmd = AddChannelCommand::new(0, snapshot)
        .execute(&mut world)
        .unwrap();
    assert_eq!(get_channel_order(&mut world).len(), 1);

    // Delete (undo add)
    let add_cmd = delete_cmd.execute(&mut world).unwrap();
    assert_eq!(get_channel_order(&mut world).len(), 0);

    // Re-add (redo add)
    let delete_cmd = add_cmd.execute(&mut world).unwrap();
    assert_eq!(get_channel_order(&mut world).len(), 1);
    assert!(id.find_entity(&mut world).is_some());

    // Re-delete (undo again)
    delete_cmd.execute(&mut world);
    assert_eq!(get_channel_order(&mut world).len(), 0);
    assert!(id.find_entity(&mut world).is_none());
}

#[test]
fn delete_channel_with_data() {
    let mut world = setup_world();
    let snapshot = ChannelSnapshot {
        data: Some(ChannelData {
            plugin_id: "com.test.plugin".to_owned(),
            plugin_state: Some("dGVzdA==".to_owned()),
        }),
        ..Default::default()
    };
    let id = snapshot.id;

    AddChannelCommand::new(0, snapshot).execute(&mut world);

    let undo = DeleteChannelCommand::new(id, 0)
        .execute(&mut world)
        .unwrap();
    undo.execute(&mut world);

    let entity = id.find_entity(&mut world).unwrap();
    let data = world.get::<ChannelData>(entity).unwrap();
    assert_eq!(data.plugin_id, "com.test.plugin");
    assert_eq!(data.plugin_state.as_deref(), Some("dGVzdA=="));
}

#[test]
fn move_forward() {
    let (mut world, [a, b, c, d]) = setup_world_with_4_channels();
    MoveChannelCommand::new(0, 2).execute(&mut world);
    assert_eq!(get_channel_ids(&mut world), vec![b, a, c, d]);
}

#[test]
fn move_backward() {
    let (mut world, [a, b, c, d]) = setup_world_with_4_channels();
    MoveChannelCommand::new(2, 0).execute(&mut world);
    assert_eq!(get_channel_ids(&mut world), vec![c, a, b, d]);
}

#[test]
fn move_forward_undo() {
    let (mut world, [a, b, c, d]) = setup_world_with_4_channels();
    let undo = MoveChannelCommand::new(0, 2).execute(&mut world).unwrap();
    undo.execute(&mut world);
    assert_eq!(get_channel_ids(&mut world), vec![a, b, c, d]);
}

#[test]
fn move_backward_undo() {
    let (mut world, [a, b, c, d]) = setup_world_with_4_channels();
    let undo = MoveChannelCommand::new(2, 0).execute(&mut world).unwrap();
    undo.execute(&mut world);
    assert_eq!(get_channel_ids(&mut world), vec![a, b, c, d]);
}

#[test]
fn move_roundtrip() {
    let (mut world, [a, b, c, d]) = setup_world_with_4_channels();

    // move(1, 3): [A, B, C, D] → [A, C, B, D]
    let undo = MoveChannelCommand::new(1, 3).execute(&mut world).unwrap();
    assert_eq!(get_channel_ids(&mut world), vec![a, c, b, d]);

    // undo
    let redo = undo.execute(&mut world).unwrap();
    assert_eq!(get_channel_ids(&mut world), vec![a, b, c, d]);

    // redo
    let undo = redo.execute(&mut world).unwrap();
    assert_eq!(get_channel_ids(&mut world), vec![a, c, b, d]);

    // undo again
    undo.execute(&mut world);
    assert_eq!(get_channel_ids(&mut world), vec![a, b, c, d]);
}

#[test]
fn move_same_position() {
    let (mut world, [a, b, c, d]) = setup_world_with_4_channels();
    MoveChannelCommand::new(1, 1).execute(&mut world);
    assert_eq!(get_channel_ids(&mut world), vec![a, b, c, d]);
}

#[test]
fn move_to_beginning() {
    let (mut world, [a, b, c, d]) = setup_world_with_4_channels();
    let undo = MoveChannelCommand::new(3, 0).execute(&mut world).unwrap();
    assert_eq!(get_channel_ids(&mut world), vec![d, a, b, c]);
    undo.execute(&mut world);
    assert_eq!(get_channel_ids(&mut world), vec![a, b, c, d]);
}

#[test]
fn move_to_end() {
    let (mut world, [a, b, c, d]) = setup_world_with_4_channels();
    let undo = MoveChannelCommand::new(0, 4).execute(&mut world).unwrap();
    assert_eq!(get_channel_ids(&mut world), vec![b, c, d, a]);
    undo.execute(&mut world);
    assert_eq!(get_channel_ids(&mut world), vec![a, b, c, d]);
}

#[test]
fn set_plugin() {
    let mut world = setup_world();
    let snapshot = ChannelSnapshot::default();
    let id = snapshot.id;
    AddChannelCommand::new(0, snapshot).execute(&mut world);

    let data = make_channel_data("com.test.synth");
    SetPluginCommand::new(id, Some(data)).execute(&mut world);

    let entity = id.find_entity(&mut world).unwrap();
    let data = world.get::<ChannelData>(entity).unwrap();
    assert_eq!(data.plugin_id, "com.test.synth");
}

#[test]
fn set_plugin_undo() {
    let mut world = setup_world();
    let snapshot = ChannelSnapshot::default();
    let id = snapshot.id;
    AddChannelCommand::new(0, snapshot).execute(&mut world);

    let data = make_channel_data("com.test.synth");
    let undo = SetPluginCommand::new(id, Some(data))
        .execute(&mut world)
        .unwrap();
    undo.execute(&mut world);

    let entity = id.find_entity(&mut world).unwrap();
    assert!(world.get::<ChannelData>(entity).is_none());
}

#[test]
fn set_plugin_replace() {
    let mut world = setup_world();
    let data_a = make_channel_data("com.test.synth-a");
    let snapshot = ChannelSnapshot {
        data: Some(data_a),
        ..Default::default()
    };
    let id = snapshot.id;
    AddChannelCommand::new(0, snapshot).execute(&mut world);

    let data_b = make_channel_data("com.test.synth-b");
    let undo = SetPluginCommand::new(id, Some(data_b))
        .execute(&mut world)
        .unwrap();

    let entity = id.find_entity(&mut world).unwrap();
    assert_eq!(
        world.get::<ChannelData>(entity).unwrap().plugin_id,
        "com.test.synth-b"
    );

    undo.execute(&mut world);
    assert_eq!(
        world.get::<ChannelData>(entity).unwrap().plugin_id,
        "com.test.synth-a"
    );
}

#[test]
fn set_plugin_roundtrip() {
    let mut world = setup_world();
    let snapshot = ChannelSnapshot::default();
    let id = snapshot.id;
    AddChannelCommand::new(0, snapshot).execute(&mut world);

    let data = make_channel_data("com.test.synth");
    let undo = SetPluginCommand::new(id, Some(data))
        .execute(&mut world)
        .unwrap();

    let entity = id.find_entity(&mut world).unwrap();
    assert!(world.get::<ChannelData>(entity).is_some());

    // undo
    let redo = undo.execute(&mut world).unwrap();
    assert!(world.get::<ChannelData>(entity).is_none());

    // redo
    let undo = redo.execute(&mut world).unwrap();
    assert_eq!(
        world.get::<ChannelData>(entity).unwrap().plugin_id,
        "com.test.synth"
    );

    // undo again
    undo.execute(&mut world);
    assert!(world.get::<ChannelData>(entity).is_none());
}

#[test]
fn set_gain() {
    let mut world = setup_world();
    let snapshot = ChannelSnapshot::default();
    let id = snapshot.id;
    AddChannelCommand::new(0, snapshot).execute(&mut world);

    SetGainCommand::new(id, 0.5).execute(&mut world);

    let entity = id.find_entity(&mut world).unwrap();
    assert_eq!(world.get::<ChannelState>(entity).unwrap().gain_value, 0.5);
}

#[test]
fn set_gain_undo() {
    let mut world = setup_world();
    let snapshot = ChannelSnapshot::default();
    let id = snapshot.id;
    AddChannelCommand::new(0, snapshot).execute(&mut world);

    let undo = SetGainCommand::new(id, 0.3).execute(&mut world).unwrap();
    undo.execute(&mut world);

    let entity = id.find_entity(&mut world).unwrap();
    assert_eq!(world.get::<ChannelState>(entity).unwrap().gain_value, 1.0);
}

#[test]
fn set_gain_roundtrip() {
    let mut world = setup_world();
    let snapshot = ChannelSnapshot::default();
    let id = snapshot.id;
    AddChannelCommand::new(0, snapshot).execute(&mut world);

    let entity = id.find_entity(&mut world).unwrap();

    let undo = SetGainCommand::new(id, 0.7).execute(&mut world).unwrap();
    assert_eq!(world.get::<ChannelState>(entity).unwrap().gain_value, 0.7);

    // undo
    let redo = undo.execute(&mut world).unwrap();
    assert_eq!(world.get::<ChannelState>(entity).unwrap().gain_value, 1.0);

    // redo
    let undo = redo.execute(&mut world).unwrap();
    assert_eq!(world.get::<ChannelState>(entity).unwrap().gain_value, 0.7);

    // undo again
    undo.execute(&mut world);
    assert_eq!(world.get::<ChannelState>(entity).unwrap().gain_value, 1.0);
}
