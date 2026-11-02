use std::sync::Arc;

use arrow::array::{ArrayRef, BooleanArray, Float32Array, Int32Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;

pub fn build(n_events: usize) -> Result<(Arc<Schema>, RecordBatch), Box<dyn std::error::Error>> {
    let mut tick_ids = Vec::with_capacity(n_events);
    let mut player_ids = Vec::with_capacity(n_events);
    let mut event_types = Vec::with_capacity(n_events);
    let mut pos_x = Vec::with_capacity(n_events);
    let mut pos_y = Vec::with_capacity(n_events);
    let mut pos_z = Vec::with_capacity(n_events);
    let mut is_headshot = Vec::with_capacity(n_events);
    let mut damage_dealt = Vec::with_capacity(n_events);

    let event_names = ["move", "fire", "hit", "kill", "jump"];
    for i in 0..n_events {
        let p_id = (i % 16) as i32;
        tick_ids.push(i as i64);
        player_ids.push(p_id);
        event_types.push(event_names[i % 5]);
        pos_x.push(10.0 + (i as f32 % 100.0));
        pos_y.push(0.0 + (i as f32 % 10.0));
        pos_z.push(20.0 + (i as f32 % 80.0));
        is_headshot.push(i % 7 == 0);
        damage_dealt.push(if i % 5 == 2 {
            45
        } else if i % 5 == 3 {
            100
        } else {
            0
        });
    }

    let schema = Arc::new(Schema::new(vec![
        Field::new("tick", DataType::Int64, false),
        Field::new("player_id", DataType::Int32, false),
        Field::new("event", DataType::Utf8, false),
        Field::new("x", DataType::Float32, false),
        Field::new("y", DataType::Float32, false),
        Field::new("z", DataType::Float32, false),
        Field::new("is_headshot", DataType::Boolean, false),
        Field::new("damage", DataType::Int32, false),
    ]));

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(tick_ids)) as ArrayRef,
            Arc::new(Int32Array::from(player_ids)) as ArrayRef,
            Arc::new(StringArray::from(event_types)) as ArrayRef,
            Arc::new(Float32Array::from(pos_x)) as ArrayRef,
            Arc::new(Float32Array::from(pos_y)) as ArrayRef,
            Arc::new(Float32Array::from(pos_z)) as ArrayRef,
            Arc::new(BooleanArray::from(is_headshot)) as ArrayRef,
            Arc::new(Int32Array::from(damage_dealt)) as ArrayRef,
        ],
    )?;
    Ok((schema, batch))
}
