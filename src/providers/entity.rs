use anyhow::Result;
use ecow::eco_format;
use hitman_bin1::{
	deserialize,
	game::h3::{
		SEntityTemplatePinConnection, SEntityTemplateReference, SMatrix43, STemplateEntityBlueprint,
		STemplateEntityFactory, SVector3, ZVariant
	},
	types::resource::ZRuntimeResourceID
};
use hitman_commons::metadata::ResourceMetadata;
use tryvial::try_fn;

use crate::providers::{DiffProvider, ResourceDiff};

fn refs_equal(
	first_meta: &ResourceMetadata,
	first_external_scenes: &[i32],
	second_meta: &ResourceMetadata,
	second_external_scenes: &[i32],
	a: &SEntityTemplateReference,
	b: &SEntityTemplateReference
) -> bool {
	a.exposed_entity == b.exposed_entity
		&& first_external_scenes
			.get(a.external_scene_index as usize)
			.and_then(|&x| first_meta.references.get(x as usize))
			== second_external_scenes
				.get(b.external_scene_index as usize)
				.and_then(|&x| second_meta.references.get(x as usize))
		&& a.entity_id == b.entity_id
		&& a.entity_index == b.entity_index
}

/// X, Y, Z equal to 3 decimal places
fn vectors_equal(a: &SVector3, b: &SVector3) -> bool {
	(a.x * 1000.0).round() == (b.x * 1000.0).round()
		&& (a.y * 1000.0).round() == (b.y * 1000.0).round()
		&& (a.z * 1000.0).round() == (b.z * 1000.0).round()
}

fn values_equal(a_meta: &ResourceMetadata, a: &ZVariant, b_meta: &ResourceMetadata, b: &ZVariant) -> bool {
	if let Some(a) = a.as_ref::<SMatrix43>()
		&& let Some(b) = b.as_ref::<SMatrix43>()
	{
		vectors_equal(&a.trans, &b.trans)
			&& vectors_equal(&a.x_axis, &b.x_axis)
			&& vectors_equal(&a.y_axis, &b.y_axis)
			&& vectors_equal(&a.z_axis, &b.z_axis)
	} else if let Some(a) = a.as_ref::<ZRuntimeResourceID>()
		&& let Some(b) = b.as_ref::<ZRuntimeResourceID>()
	{
		a_meta.references.get(a.id_low as usize) == b_meta.references.get(b.id_low as usize)
	} else {
		a == b
	}
}

pub struct TEMPDiff;

inventory::submit!(&TEMPDiff as &dyn DiffProvider);

impl DiffProvider for TEMPDiff {
	fn resource_type(&self) -> &'static str {
		"TEMP"
	}

	#[try_fn]
	fn diff(
		&self,
		first_meta: &ResourceMetadata,
		first_data: &[u8],
		second_meta: &ResourceMetadata,
		second_data: &[u8]
	) -> Result<ResourceDiff> {
		let first_data: STemplateEntityFactory = deserialize(first_data)?;
		let second_data: STemplateEntityFactory = deserialize(second_data)?;

		if first_data.sub_type != second_data.sub_type {
			return Ok(ResourceDiff::Different {
				description: Some("Subtypes are different".into())
			});
		}

		if first_data.root_entity_index != second_data.root_entity_index {
			return Ok(ResourceDiff::Different {
				description: Some("Root entity indices are different".into())
			});
		}

		if first_meta
			.references
			.get(first_data.blueprint_index_in_resource_header as usize)
			!= second_meta
				.references
				.get(second_data.blueprint_index_in_resource_header as usize)
		{
			return Ok(ResourceDiff::Different {
				description: Some("Blueprints are different".into())
			});
		}

		if first_data
			.external_scene_type_indices_in_resource_header
			.iter()
			.any(|&x| {
				!second_data
					.external_scene_type_indices_in_resource_header
					.iter()
					.any(|&y| first_meta.references.get(x as usize) == second_meta.references.get(y as usize))
			}) || second_data
			.external_scene_type_indices_in_resource_header
			.iter()
			.any(|&x| {
				!first_data
					.external_scene_type_indices_in_resource_header
					.iter()
					.any(|&y| second_meta.references.get(x as usize) == first_meta.references.get(y as usize))
			}) {
			return Ok(ResourceDiff::Different {
				description: Some("External scenes are different".into())
			});
		}

		if first_data.property_overrides.iter().any(|x| {
			!second_data.property_overrides.iter().any(|y| {
				refs_equal(
					first_meta,
					&first_data.external_scene_type_indices_in_resource_header,
					second_meta,
					&second_data.external_scene_type_indices_in_resource_header,
					&x.property_owner,
					&y.property_owner
				) && x.property_value.property_id == y.property_value.property_id
					&& values_equal(
						first_meta,
						&x.property_value.value,
						second_meta,
						&y.property_value.value
					)
			})
		}) || second_data.property_overrides.iter().any(|y| {
			!first_data.property_overrides.iter().any(|x| {
				refs_equal(
					first_meta,
					&first_data.external_scene_type_indices_in_resource_header,
					second_meta,
					&second_data.external_scene_type_indices_in_resource_header,
					&x.property_owner,
					&y.property_owner
				) && x.property_value.property_id == y.property_value.property_id
					&& values_equal(
						first_meta,
						&x.property_value.value,
						second_meta,
						&y.property_value.value
					)
			})
		}) {
			return Ok(ResourceDiff::Different {
				description: Some("Property overrides are different".into())
			});
		}

		if first_data.sub_entities.len() != second_data.sub_entities.len() {
			return Ok(ResourceDiff::Different {
				description: Some("Sub-entity counts are different".into())
			});
		}

		for (idx, (ent1, ent2)) in first_data
			.sub_entities
			.iter()
			.zip(&second_data.sub_entities)
			.enumerate()
		{
			if !refs_equal(
				first_meta,
				&first_data.external_scene_type_indices_in_resource_header,
				second_meta,
				&second_data.external_scene_type_indices_in_resource_header,
				&ent1.logical_parent,
				&ent2.logical_parent
			) {
				return Ok(ResourceDiff::Different {
					description: Some(eco_format!("Logical parents are different for entity {idx}"))
				});
			}

			if first_meta.references.get(ent1.entity_type_resource_index as usize)
				!= second_meta.references.get(ent2.entity_type_resource_index as usize)
			{
				return Ok(ResourceDiff::Different {
					description: Some(eco_format!("Entity factories are different for entity {idx}"))
				});
			}

			if ent1.property_values.iter().any(|x| {
				!ent2.property_values.iter().any(|y| {
					x.property_id == y.property_id && values_equal(first_meta, &x.value, second_meta, &y.value)
				})
			}) || ent2.property_values.iter().any(|y| {
				!ent1.property_values.iter().any(|x| {
					x.property_id == y.property_id && values_equal(first_meta, &x.value, second_meta, &y.value)
				})
			}) {
				return Ok(ResourceDiff::Different {
					description: Some(eco_format!("Property values are different for entity {idx}"))
				});
			}

			if ent1.post_init_property_values.iter().any(|x| {
				!ent2.post_init_property_values.iter().any(|y| {
					x.property_id == y.property_id && values_equal(first_meta, &x.value, second_meta, &y.value)
				})
			}) || ent2.post_init_property_values.iter().any(|y| {
				!ent1.post_init_property_values.iter().any(|x| {
					x.property_id == y.property_id && values_equal(first_meta, &x.value, second_meta, &y.value)
				})
			}) {
				return Ok(ResourceDiff::Different {
					description: Some(eco_format!("Post-init property values are different for entity {idx}"))
				});
			}

			if ent1.platform_specific_property_values.iter().any(|x| {
				!ent2.platform_specific_property_values.iter().any(|y| {
					x.platform == y.platform
						&& x.post_init == y.post_init
						&& x.property_value.property_id == y.property_value.property_id
						&& values_equal(
							first_meta,
							&x.property_value.value,
							second_meta,
							&y.property_value.value
						)
				})
			}) || ent2.platform_specific_property_values.iter().any(|y| {
				!ent1.platform_specific_property_values.iter().any(|x| {
					x.platform == y.platform
						&& x.post_init == y.post_init
						&& x.property_value.property_id == y.property_value.property_id
						&& values_equal(
							first_meta,
							&x.property_value.value,
							second_meta,
							&y.property_value.value
						)
				})
			}) {
				return Ok(ResourceDiff::Different {
					description: Some(eco_format!(
						"Platform-specific property values are different for entity {idx}"
					))
				});
			}
		}

		ResourceDiff::Same
	}

	#[try_fn]
	fn extract(&self, meta: &ResourceMetadata, data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
		let data: STemplateEntityFactory = deserialize(data)?;

		vec![(
			format!("{}.{}.json", meta.id.to_hash(), meta.resource_type),
			serde_json::to_vec_pretty(&data)?
		)]
	}
}

pub struct TBLUDiff;

inventory::submit!(&TBLUDiff as &dyn DiffProvider);

impl DiffProvider for TBLUDiff {
	fn resource_type(&self) -> &'static str {
		"TBLU"
	}

	#[try_fn]
	fn diff(
		&self,
		first_meta: &ResourceMetadata,
		first_data: &[u8],
		second_meta: &ResourceMetadata,
		second_data: &[u8]
	) -> Result<ResourceDiff> {
		let first_data: STemplateEntityBlueprint = deserialize(first_data)?;
		let second_data: STemplateEntityBlueprint = deserialize(second_data)?;

		if first_data.sub_type != second_data.sub_type {
			return Ok(ResourceDiff::Different {
				description: Some("Subtypes are different".into())
			});
		}

		if first_data.root_entity_index != second_data.root_entity_index {
			return Ok(ResourceDiff::Different {
				description: Some("Root entity indices are different".into())
			});
		}

		if first_data
			.external_scene_type_indices_in_resource_header
			.iter()
			.any(|&x| {
				!second_data
					.external_scene_type_indices_in_resource_header
					.iter()
					.any(|&y| first_meta.references.get(x as usize) == second_meta.references.get(y as usize))
			}) || second_data
			.external_scene_type_indices_in_resource_header
			.iter()
			.any(|&y| {
				!first_data
					.external_scene_type_indices_in_resource_header
					.iter()
					.any(|&x| first_meta.references.get(x as usize) == second_meta.references.get(y as usize))
			}) {
			return Ok(ResourceDiff::Different {
				description: Some("External scenes are different".into())
			});
		}

		let pin_connections_equal = |a: &SEntityTemplatePinConnection, b: &SEntityTemplatePinConnection| {
			a.from_id == b.from_id
				&& a.from_pin_name == b.from_pin_name
				&& a.to_id == b.to_id
				&& a.to_pin_name == b.to_pin_name
				&& values_equal(first_meta, &a.constant_pin_value, second_meta, &b.constant_pin_value)
		};

		if first_data
			.pin_connections
			.iter()
			.any(|x| !second_data.pin_connections.iter().any(|y| pin_connections_equal(x, y)))
			|| second_data
				.pin_connections
				.iter()
				.any(|y| !first_data.pin_connections.iter().any(|x| pin_connections_equal(x, y)))
		{
			return Ok(ResourceDiff::Different {
				description: Some("Pin connections are different".into())
			});
		}

		if first_data.input_pin_forwardings.iter().any(|x| {
			!second_data
				.input_pin_forwardings
				.iter()
				.any(|y| pin_connections_equal(x, y))
		}) || second_data.input_pin_forwardings.iter().any(|y| {
			!first_data
				.input_pin_forwardings
				.iter()
				.any(|x| pin_connections_equal(x, y))
		}) {
			return Ok(ResourceDiff::Different {
				description: Some("Input pin forwardings are different".into())
			});
		}

		if first_data.output_pin_forwardings.iter().any(|x| {
			!second_data
				.output_pin_forwardings
				.iter()
				.any(|y| pin_connections_equal(x, y))
		}) || second_data.output_pin_forwardings.iter().any(|y| {
			!first_data
				.output_pin_forwardings
				.iter()
				.any(|x| pin_connections_equal(x, y))
		}) {
			return Ok(ResourceDiff::Different {
				description: Some("Output pin forwardings are different".into())
			});
		}

		if first_data.override_deletes.iter().any(|x| {
			!second_data.override_deletes.iter().any(|y| {
				refs_equal(
					first_meta,
					&first_data.external_scene_type_indices_in_resource_header,
					second_meta,
					&second_data.external_scene_type_indices_in_resource_header,
					x,
					y
				)
			})
		}) || second_data.override_deletes.iter().any(|y| {
			!first_data.override_deletes.iter().any(|x| {
				refs_equal(
					first_meta,
					&first_data.external_scene_type_indices_in_resource_header,
					second_meta,
					&second_data.external_scene_type_indices_in_resource_header,
					x,
					y
				)
			})
		}) {
			return Ok(ResourceDiff::Different {
				description: Some("Override deletes are different".into())
			});
		}

		if first_data.pin_connection_overrides.iter().any(|x| {
			!second_data.pin_connection_overrides.iter().any(|y| {
				refs_equal(
					first_meta,
					&first_data.external_scene_type_indices_in_resource_header,
					second_meta,
					&second_data.external_scene_type_indices_in_resource_header,
					&x.from_entity,
					&y.from_entity
				) && refs_equal(
					first_meta,
					&first_data.external_scene_type_indices_in_resource_header,
					second_meta,
					&second_data.external_scene_type_indices_in_resource_header,
					&x.to_entity,
					&y.to_entity
				) && x.from_pin_name == y.from_pin_name
					&& x.to_pin_name == y.to_pin_name
					&& values_equal(first_meta, &x.constant_pin_value, second_meta, &y.constant_pin_value)
			})
		}) || second_data.pin_connection_overrides.iter().any(|y| {
			!first_data.pin_connection_overrides.iter().any(|x| {
				refs_equal(
					first_meta,
					&first_data.external_scene_type_indices_in_resource_header,
					second_meta,
					&second_data.external_scene_type_indices_in_resource_header,
					&x.from_entity,
					&y.from_entity
				) && refs_equal(
					first_meta,
					&first_data.external_scene_type_indices_in_resource_header,
					second_meta,
					&second_data.external_scene_type_indices_in_resource_header,
					&x.to_entity,
					&y.to_entity
				) && x.from_pin_name == y.from_pin_name
					&& x.to_pin_name == y.to_pin_name
					&& values_equal(first_meta, &x.constant_pin_value, second_meta, &y.constant_pin_value)
			})
		}) {
			return Ok(ResourceDiff::Different {
				description: Some("Pin connection overrides are different".into())
			});
		}

		if first_data.pin_connection_override_deletes.iter().any(|x| {
			!second_data.pin_connection_override_deletes.iter().any(|y| {
				refs_equal(
					first_meta,
					&first_data.external_scene_type_indices_in_resource_header,
					second_meta,
					&second_data.external_scene_type_indices_in_resource_header,
					&x.from_entity,
					&y.from_entity
				) && refs_equal(
					first_meta,
					&first_data.external_scene_type_indices_in_resource_header,
					second_meta,
					&second_data.external_scene_type_indices_in_resource_header,
					&x.to_entity,
					&y.to_entity
				) && x.from_pin_name == y.from_pin_name
					&& x.to_pin_name == y.to_pin_name
					&& values_equal(first_meta, &x.constant_pin_value, second_meta, &y.constant_pin_value)
			})
		}) || second_data.pin_connection_override_deletes.iter().any(|y| {
			!first_data.pin_connection_override_deletes.iter().any(|x| {
				refs_equal(
					first_meta,
					&first_data.external_scene_type_indices_in_resource_header,
					second_meta,
					&second_data.external_scene_type_indices_in_resource_header,
					&x.from_entity,
					&y.from_entity
				) && refs_equal(
					first_meta,
					&first_data.external_scene_type_indices_in_resource_header,
					second_meta,
					&second_data.external_scene_type_indices_in_resource_header,
					&x.to_entity,
					&y.to_entity
				) && x.from_pin_name == y.from_pin_name
					&& x.to_pin_name == y.to_pin_name
					&& values_equal(first_meta, &x.constant_pin_value, second_meta, &y.constant_pin_value)
			})
		}) {
			return Ok(ResourceDiff::Different {
				description: Some("Pin connection override deletes are different".into())
			});
		}

		for (idx, (ent1, ent2)) in first_data
			.sub_entities
			.iter()
			.zip(&second_data.sub_entities)
			.enumerate()
		{
			if !refs_equal(
				first_meta,
				&first_data.external_scene_type_indices_in_resource_header,
				second_meta,
				&second_data.external_scene_type_indices_in_resource_header,
				&ent1.logical_parent,
				&ent2.logical_parent
			) {
				return Ok(ResourceDiff::Different {
					description: Some(eco_format!("Logical parents are different for entity {idx}"))
				});
			}

			if first_meta.references.get(ent1.entity_type_resource_index as usize)
				!= second_meta.references.get(ent2.entity_type_resource_index as usize)
			{
				return Ok(ResourceDiff::Different {
					description: Some(eco_format!("Entity blueprints are different for entity {idx}"))
				});
			}

			if ent1.editor_only != ent2.editor_only {
				return Ok(ResourceDiff::Different {
					description: Some(eco_format!("Editor only flags are different for entity {idx}"))
				});
			}

			if ent1.entity_id != ent2.entity_id {
				return Ok(ResourceDiff::Different {
					description: Some(eco_format!("Entity IDs are different for entity {idx}"))
				});
			}

			if ent1.entity_name != ent2.entity_name {
				return Ok(ResourceDiff::Different {
					description: Some(eco_format!("Entity names are different for entity {idx}"))
				});
			}

			if ent1.entity_subsets.iter().any(|x| {
				!ent2.entity_subsets.iter().any(|y| {
					x.0 == y.0
						&& x.1.entities.iter().all(|&x| {
							y.1.entities.iter().any(|&y| {
								first_meta.references.get(x as usize) == second_meta.references.get(y as usize)
							})
						}) && y.1.entities.iter().all(|&y| {
						x.1.entities
							.iter()
							.any(|&x| first_meta.references.get(x as usize) == second_meta.references.get(y as usize))
					})
				})
			}) || ent2.entity_subsets.iter().any(|y| {
				!ent1.entity_subsets.iter().any(|x| {
					x.0 == y.0
						&& x.1.entities.iter().all(|&x| {
							y.1.entities.iter().any(|&y| {
								first_meta.references.get(x as usize) == second_meta.references.get(y as usize)
							})
						}) && y.1.entities.iter().all(|&y| {
						x.1.entities
							.iter()
							.any(|&x| first_meta.references.get(x as usize) == second_meta.references.get(y as usize))
					})
				})
			}) {
				return Ok(ResourceDiff::Different {
					description: Some(eco_format!("Entity subsets are different for entity {idx}"))
				});
			}

			if ent1.exposed_interfaces.iter().any(|x| {
				!ent2.exposed_interfaces.iter().any(|y| {
					x.0 == y.0 && first_meta.references.get(x.1 as usize) == second_meta.references.get(y.1 as usize)
				})
			}) || ent2.exposed_interfaces.iter().any(|x| {
				!ent1.exposed_interfaces.iter().any(|y| {
					x.0 == y.0 && first_meta.references.get(x.1 as usize) == second_meta.references.get(y.1 as usize)
				})
			}) {
				return Ok(ResourceDiff::Different {
					description: Some(eco_format!("Exposed interfaces are different for entity {idx}"))
				});
			}

			if ent1.exposed_entities.iter().any(|x| {
				!ent2.exposed_entities.iter().any(|y| {
					x.is_array == y.is_array
						&& x.name == y.name
						&& x.targets.iter().all(|x| {
							y.targets.iter().any(|y| {
								refs_equal(
									first_meta,
									&first_data.external_scene_type_indices_in_resource_header,
									second_meta,
									&second_data.external_scene_type_indices_in_resource_header,
									x,
									y
								)
							})
						}) && y.targets.iter().all(|y| {
						x.targets.iter().any(|x| {
							refs_equal(
								first_meta,
								&first_data.external_scene_type_indices_in_resource_header,
								second_meta,
								&second_data.external_scene_type_indices_in_resource_header,
								x,
								y
							)
						})
					})
				})
			}) || ent2.exposed_entities.iter().any(|y| {
				!ent1.exposed_entities.iter().any(|x| {
					x.is_array == y.is_array
						&& x.name == y.name
						&& x.targets.iter().all(|x| {
							y.targets.iter().any(|y| {
								refs_equal(
									first_meta,
									&first_data.external_scene_type_indices_in_resource_header,
									second_meta,
									&second_data.external_scene_type_indices_in_resource_header,
									x,
									y
								)
							})
						}) && y.targets.iter().all(|y| {
						x.targets.iter().any(|x| {
							refs_equal(
								first_meta,
								&first_data.external_scene_type_indices_in_resource_header,
								second_meta,
								&second_data.external_scene_type_indices_in_resource_header,
								x,
								y
							)
						})
					})
				})
			}) {
				return Ok(ResourceDiff::Different {
					description: Some(eco_format!("Exposed entities are different for entity {idx}"))
				});
			}

			if ent1.property_aliases.iter().any(|x| {
				!ent2.property_aliases.iter().any(|y| {
					x.alias_name == y.alias_name
						&& x.property_name == y.property_name
						&& first_meta.references.get(x.entity_id as usize)
							== second_meta.references.get(y.entity_id as usize)
				})
			}) || ent2.property_aliases.iter().any(|y| {
				!ent1.property_aliases.iter().any(|x| {
					x.alias_name == y.alias_name
						&& x.property_name == y.property_name
						&& first_meta.references.get(x.entity_id as usize)
							== second_meta.references.get(y.entity_id as usize)
				})
			}) {
				return Ok(ResourceDiff::Different {
					description: Some(eco_format!("Property aliases are different for entity {idx}"))
				});
			}
		}

		ResourceDiff::Same
	}

	#[try_fn]
	fn extract(&self, meta: &ResourceMetadata, data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
		let data: STemplateEntityBlueprint = deserialize(data)?;

		vec![(
			format!("{}.{}.json", meta.id.to_hash(), meta.resource_type),
			serde_json::to_vec_pretty(&data)?
		)]
	}
}
