use std::collections::HashMap;

use data_core::ObservationModelPack;
use geometry_core::{SlotId, SlotLattice, SlotObservation};

pub(super) fn calibrate_observation_lattice(
    lattice: &SlotLattice,
    distance_unit: f32,
    slot_units: &HashMap<SlotId, f32>,
) -> SlotLattice {
    let unit = distance_unit.max(0.0001);
    SlotLattice::new(
        lattice
            .positions
            .iter()
            .map(|position| {
                position
                    .iter()
                    .map(|slot| SlotObservation {
                        slot_id: slot.slot_id.clone(),
                        cost: slot.cost
                            / slot_units
                                .get(&slot.slot_id)
                                .copied()
                                .unwrap_or(unit)
                                .max(0.0001),
                    })
                    .collect()
            })
            .collect(),
    )
}

pub(super) fn observation_distance_unit_from_pack(pack: &ObservationModelPack) -> Option<f32> {
    ["endpoint_error", "startpoint_error", "step_length"]
        .into_iter()
        .find_map(|name| {
            let mean = pack.error_model.features.get(name)?.mean;
            (mean.is_finite() && mean > 0.0).then_some(mean as f32)
        })
}

pub(super) fn observation_slot_units_from_pack(
    pack: &ObservationModelPack,
) -> HashMap<SlotId, f32> {
    pack.error_model
        .slot_errors
        .iter()
        .filter_map(|(slot_id, stats)| {
            let unit = stats
                .endpoint_error
                .as_ref()
                .or(stats.startpoint_error.as_ref())?
                .mean;
            (unit.is_finite() && unit > 0.0).then(|| (SlotId::new(slot_id), unit as f32))
        })
        .collect()
}
