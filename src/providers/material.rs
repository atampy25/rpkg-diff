use std::io::{Cursor, Read};

use anyhow::{Result, bail};
use hitman_commons::metadata::ResourceMetadata;
use hitman_formats::material::{MaterialError, MaterialInstance, MaterialOverride};
use serde_json::to_vec_pretty;
use tryvial::try_fn;

use crate::providers::{DiffProvider, ResourceDiff};

#[try_fn]
fn convert_matt(matt_metadata: &ResourceMetadata, matt_data: &[u8]) -> Result<Vec<MaterialOverride>> {
	let mut matt = Cursor::new(matt_data);

	let mut properties = vec![];

	while matt.position() < matt.get_ref().len().saturating_sub(1) as u64 {
		let entry_type = {
			let mut x = [0u8; 1];
			matt.read_exact(&mut x)?;
			x[0]
		};

		properties.push(match entry_type {
			// A texture.
			1 => {
				let texture_dependency_index = u32::from_le_bytes({
					let mut x = [0u8; 4];
					matt.read_exact(&mut x)?;
					x
				});

				if texture_dependency_index != u32::MAX {
					MaterialOverride::Texture(Some(
						matt_metadata
							.references
							.get(usize::try_from(texture_dependency_index)?)
							.ok_or_else(|| {
								MaterialError::InvalidDependency(usize::try_from(texture_dependency_index).unwrap())
							})?
							.resource
							.to_owned()
					))
				} else {
					MaterialOverride::Texture(None)
				}
			}

			// An RGB colour.
			2 => {
				let x = f32::from_le_bytes({
					let mut x = [0u8; 4];
					matt.read_exact(&mut x)?;
					x
				});

				let y = f32::from_le_bytes({
					let mut x = [0u8; 4];
					matt.read_exact(&mut x)?;
					x
				});

				let z = f32::from_le_bytes({
					let mut x = [0u8; 4];
					matt.read_exact(&mut x)?;
					x
				});

				MaterialOverride::Color(format!(
					"#{:0>2x}{:0>2x}{:0>2x}",
					(x * 255.0).round() as u8,
					(y * 255.0).round() as u8,
					(z * 255.0).round() as u8
				))
			}

			// An RGBA colour.
			3 => {
				let x = f32::from_le_bytes({
					let mut x = [0u8; 4];
					matt.read_exact(&mut x)?;
					x
				});

				let y = f32::from_le_bytes({
					let mut x = [0u8; 4];
					matt.read_exact(&mut x)?;
					x
				});

				let z = f32::from_le_bytes({
					let mut x = [0u8; 4];
					matt.read_exact(&mut x)?;
					x
				});

				let w = f32::from_le_bytes({
					let mut x = [0u8; 4];
					matt.read_exact(&mut x)?;
					x
				});

				MaterialOverride::Color(format!(
					"#{:0>2x}{:0>2x}{:0>2x}{:0>2x}",
					(x * 255.0).round() as u8,
					(y * 255.0).round() as u8,
					(z * 255.0).round() as u8,
					(w * 255.0).round() as u8
				))
			}

			// A float.
			4 => {
				let val = f32::from_le_bytes({
					let mut x = [0u8; 4];
					matt.read_exact(&mut x)?;
					x
				});

				MaterialOverride::Float(val)
			}

			// A Vector2.
			5 => {
				let x = f32::from_le_bytes({
					let mut x = [0u8; 4];
					matt.read_exact(&mut x)?;
					x
				});

				let y = f32::from_le_bytes({
					let mut x = [0u8; 4];
					matt.read_exact(&mut x)?;
					x
				});

				MaterialOverride::Vector(vec![x, y])
			}

			// A Vector3.
			6 => {
				let x = f32::from_le_bytes({
					let mut x = [0u8; 4];
					matt.read_exact(&mut x)?;
					x
				});

				let y = f32::from_le_bytes({
					let mut x = [0u8; 4];
					matt.read_exact(&mut x)?;
					x
				});

				let z = f32::from_le_bytes({
					let mut x = [0u8; 4];
					matt.read_exact(&mut x)?;
					x
				});

				MaterialOverride::Vector(vec![x, y, z])
			}

			// A Vector4.
			7 => {
				let x = f32::from_le_bytes({
					let mut x = [0u8; 4];
					matt.read_exact(&mut x)?;
					x
				});

				let y = f32::from_le_bytes({
					let mut x = [0u8; 4];
					matt.read_exact(&mut x)?;
					x
				});

				let z = f32::from_le_bytes({
					let mut x = [0u8; 4];
					matt.read_exact(&mut x)?;
					x
				});

				let w = f32::from_le_bytes({
					let mut x = [0u8; 4];
					matt.read_exact(&mut x)?;
					x
				});

				MaterialOverride::Vector(vec![x, y, z, w])
			}

			_ => bail!(MaterialError::UnrecognisedEntryType(entry_type))
		});
	}

	properties
}

pub struct MATTDiff;

inventory::submit!(&MATTDiff as &dyn DiffProvider);

impl DiffProvider for MATTDiff {
	fn resource_type(&self) -> &'static str {
		"MATT"
	}

	#[try_fn]
	fn diff(
		&self,
		first_meta: &ResourceMetadata,
		first_data: &[u8],
		second_meta: &ResourceMetadata,
		second_data: &[u8]
	) -> Result<ResourceDiff> {
		if first_meta.scrambled != second_meta.scrambled || first_meta.references != second_meta.references {
			return Ok(ResourceDiff::Different {
				description: Some("Material metadata is different".into())
			});
		}

		if convert_matt(first_meta, first_data)? != convert_matt(second_meta, second_data)? {
			return Ok(ResourceDiff::Different {
				description: Some("Material overrides are different".into())
			});
		}

		ResourceDiff::Same
	}

	#[try_fn]
	fn extract(&self, meta: &ResourceMetadata, data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
		vec![(
			format!("{}.{}.json", meta.id.to_hash(), meta.resource_type),
			serde_json::to_vec_pretty(&convert_matt(meta, data)?)?
		)]
	}
}

#[try_fn]
fn convert_matb(matb_data: &[u8]) -> Result<Vec<(u8, String)>> {
	let mut matb = Cursor::new(matb_data);

	let mut prop_names = vec![];

	while matb.position() < matb.get_ref().len().saturating_sub(1) as u64 {
		let entry_type = {
			let mut x = [0u8; 1];
			matb.read_exact(&mut x)?;
			x[0]
		};

		let matb_string_length = u32::from_le_bytes({
			let mut x = [0u8; 4];
			matb.read_exact(&mut x)?;
			x
		});

		// I'm assuming that no one is using a 16-bit computer
		let mut string_data = vec![0; matb_string_length as usize];
		matb.read_exact(&mut string_data)?;

		prop_names.push((
			entry_type,
			std::str::from_utf8(&string_data[0..string_data.len() - 1])?.to_owned()
		));
	}

	prop_names
}

pub struct MATBDiff;

inventory::submit!(&MATBDiff as &dyn DiffProvider);

impl DiffProvider for MATBDiff {
	fn resource_type(&self) -> &'static str {
		"MATB"
	}

	#[try_fn]
	fn diff(
		&self,
		first_meta: &ResourceMetadata,
		first_data: &[u8],
		second_meta: &ResourceMetadata,
		second_data: &[u8]
	) -> Result<ResourceDiff> {
		if first_meta.scrambled != second_meta.scrambled || first_meta.references != second_meta.references {
			return Ok(ResourceDiff::Different {
				description: Some("Material metadata is different".into())
			});
		}

		if convert_matb(first_data)? != convert_matb(second_data)? {
			return Ok(ResourceDiff::Different {
				description: Some("Material overrides are different".into())
			});
		}

		ResourceDiff::Same
	}

	#[try_fn]
	fn extract(&self, meta: &ResourceMetadata, data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
		vec![(
			format!("{}.{}.json", meta.id.to_hash(), meta.resource_type),
			serde_json::to_vec_pretty(&convert_matb(data)?)?
		)]
	}
}

pub struct MATIDiff;

inventory::submit!(&MATIDiff as &dyn DiffProvider);

impl DiffProvider for MATIDiff {
	fn resource_type(&self) -> &'static str {
		"MATI"
	}

	#[try_fn]
	fn diff(
		&self,
		first_meta: &ResourceMetadata,
		first_data: &[u8],
		second_meta: &ResourceMetadata,
		second_data: &[u8]
	) -> Result<ResourceDiff> {
		let first_data = MaterialInstance::parse(first_data, first_meta)?;
		let second_data = MaterialInstance::parse(second_data, second_meta)?;

		if first_data != second_data {
			ResourceDiff::Different {
				description: Some("Materials are different".into())
			}
		} else {
			ResourceDiff::Same
		}
	}

	#[try_fn]
	fn extract(&self, meta: &ResourceMetadata, data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
		vec![(
			format!("{}.{}.json", meta.id.to_hash(), meta.resource_type),
			to_vec_pretty(&MaterialInstance::parse(data, meta)?)?
		)]
	}
}
