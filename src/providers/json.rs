use std::collections::HashMap;

use anyhow::Result;
use ecow::eco_format;
use hitman_commons::metadata::ResourceMetadata;
use hitman_formats::ores::{parse_hashes_ores, parse_json_ores};
use serde_json::Value;
use tryvial::try_fn;

use crate::providers::{DiffProvider, ResourceDiff};

pub struct REPODiff;

inventory::submit!(&REPODiff as &dyn DiffProvider);

impl DiffProvider for REPODiff {
	fn resource_type(&self) -> &'static str {
		"REPO"
	}

	#[try_fn]
	fn diff(
		&self,
		_: &ResourceMetadata,
		first_data: &[u8],
		_: &ResourceMetadata,
		second_data: &[u8]
	) -> Result<ResourceDiff> {
		let first_data = serde_json::from_slice::<Vec<Value>>(first_data)?;
		let first_data = first_data
			.iter()
			.map(|x| {
				(
					x.get("ID_")
						.expect("No ID_ on entry in first REPO")
						.as_str()
						.expect("ID_ was not string in first REPO"),
					x
				)
			})
			.collect::<HashMap<&str, &Value>>();

		let second_data = serde_json::from_slice::<Vec<Value>>(second_data)?;
		let second_data = second_data
			.iter()
			.map(|x| {
				(
					x.get("ID_")
						.expect("No ID_ on entry in second REPO")
						.as_str()
						.expect("ID_ was not string in second REPO"),
					x
				)
			})
			.collect::<HashMap<&str, &Value>>();

		if first_data != second_data {
			ResourceDiff::Different {
				description: {
					first_data
						.keys()
						.find(|k| !second_data.contains_key(*k))
						.map(|k| eco_format!("Entry {} only in first", k))
						.or_else(|| {
							second_data
								.keys()
								.find(|k| !first_data.contains_key(*k))
								.map(|k| eco_format!("Entry {} only in second", k))
						})
						.or_else(|| {
							first_data
								.iter()
								.find(|(k, v)| second_data.get(*k) != Some(v))
								.map(|(k, _)| eco_format!("Entry {} is different", k))
						})
				}
			}
		} else {
			ResourceDiff::Same
		}
	}
}

pub struct ORESDiff;

inventory::submit!(&ORESDiff as &dyn DiffProvider);

impl DiffProvider for ORESDiff {
	fn resource_type(&self) -> &'static str {
		"ORES"
	}

	#[try_fn]
	fn diff(
		&self,
		first_meta: &ResourceMetadata,
		first_data: &[u8],
		_: &ResourceMetadata,
		second_data: &[u8]
	) -> Result<ResourceDiff> {
		if first_meta.id.to_hash() == "0057C2C3941115CA" {
			let first_data = parse_json_ores(first_data)?;
			let second_data = parse_json_ores(second_data)?;

			let first_data = serde_json::from_str::<Vec<Value>>(&first_data)?;
			let first_data = first_data
				.iter()
				.map(|x| {
					(
						x.get("Id")
							.expect("No Id on entry in first ORES")
							.as_str()
							.expect("Id was not string in first ORES"),
						x
					)
				})
				.collect::<HashMap<&str, &Value>>();

			let second_data = serde_json::from_str::<Vec<Value>>(&second_data)?;
			let second_data = second_data
				.iter()
				.map(|x| {
					(
						x.get("Id")
							.expect("No Id on entry in second ORES")
							.as_str()
							.expect("Id was not string in second ORES"),
						x
					)
				})
				.collect::<HashMap<&str, &Value>>();

			if first_data != second_data {
				ResourceDiff::Different {
					description: {
						first_data
							.keys()
							.find(|k| !second_data.contains_key(*k))
							.map(|k| eco_format!("Entry {} only in first", k))
							.or_else(|| {
								second_data
									.keys()
									.find(|k| !first_data.contains_key(*k))
									.map(|k| eco_format!("Entry {} only in second", k))
							})
							.or_else(|| {
								first_data
									.iter()
									.find(|(k, v)| second_data.get(*k) != Some(v))
									.map(|(k, _)| eco_format!("Entry {} is different", k))
							})
					}
				}
			} else {
				ResourceDiff::Same
			}
		} else {
			let first_data = parse_hashes_ores(first_data)?;
			let second_data = parse_hashes_ores(second_data)?;

			if first_data != second_data {
				ResourceDiff::Different {
					description: {
						first_data
							.keys()
							.find(|k| !second_data.contains_key(*k))
							.map(|k| eco_format!("Entry {} only in first", k))
							.or_else(|| {
								second_data
									.keys()
									.find(|k| !first_data.contains_key(*k))
									.map(|k| eco_format!("Entry {} only in second", k))
							})
							.or_else(|| {
								first_data
									.iter()
									.find(|(k, v)| second_data.get(*k) != Some(v))
									.map(|(k, _)| eco_format!("Entry {} is different", k))
							})
					}
				}
			} else {
				ResourceDiff::Same
			}
		}
	}

	#[try_fn]
	fn extract(&self, meta: &ResourceMetadata, data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
		vec![(
			format!("{}.{}.json", meta.id.to_hash(), meta.resource_type),
			if meta.id.to_hash() == "0057C2C3941115CA" {
				parse_json_ores(data)?.into_bytes()
			} else {
				serde_json::to_vec_pretty(&parse_hashes_ores(data)?)?
			}
		)]
	}
}
