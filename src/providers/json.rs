use std::collections::HashMap;

use anyhow::Result;
use ecow::{EcoString, eco_format};
use hitman_bin1::{deserialize, game::h3::SBlobsConfigResourceEntry};
use hitman_commons::metadata::ResourceMetadata;
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
			let first_data = deserialize::<EcoString>(first_data)?;
			let second_data = deserialize::<EcoString>(second_data)?;

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
			let first_data = deserialize::<Vec<SBlobsConfigResourceEntry>>(first_data)?;
			let second_data = deserialize::<Vec<SBlobsConfigResourceEntry>>(second_data)?;

			if first_data != second_data {
				ResourceDiff::Different {
					description: {
						first_data
							.iter()
							.find(|x| !second_data.iter().any(|y| x.id == y.id))
							.map(|x| eco_format!("Entry {} only in first", x.id))
							.or_else(|| {
								second_data
									.iter()
									.find(|x| !first_data.iter().any(|y| x.id == y.id))
									.map(|x| eco_format!("Entry {} only in second", x.id))
							})
							.or_else(|| {
								first_data
									.iter()
									.find(|x| !second_data.iter().any(|y| x.id == y.id))
									.map(|x| eco_format!("Entry {} is different", x.id))
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
				deserialize::<EcoString>(data)?.as_bytes().into()
			} else {
				serde_json::to_vec_pretty(&deserialize::<Vec<SBlobsConfigResourceEntry>>(data)?)?
			}
		)]
	}
}
