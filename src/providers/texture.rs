use anyhow::{Context, Result};
use glacier_texture::{WoaVersion, convert::create_dds, mipblock::MipblockData, texture_map::TextureMap};
use hitman_commons::metadata::ResourceMetadata;
use tryvial::try_fn;

use crate::providers::{DiffProvider, ResourceDiff};

pub struct TEXTDiff;

inventory::submit!(&TEXTDiff as &dyn DiffProvider);

impl DiffProvider for TEXTDiff {
	fn resource_type(&self) -> &'static str {
		"TEXT"
	}

	#[try_fn]
	fn diff(
		&self,
		first_meta: &ResourceMetadata,
		first_data: &[u8],
		second_meta: &ResourceMetadata,
		second_data: &[u8]
	) -> Result<ResourceDiff> {
		let first_data = TextureMap::from_memory(first_data, WoaVersion::HM3).context("Parsing first TEXT")?;
		let second_data = TextureMap::from_memory(second_data, WoaVersion::HM3).context("Parsing second TEXT")?;

		if first_data.mipmaps().next().is_some_and(|x| x.is_ok())
			&& second_data.mipmaps().next().is_some_and(|x| x.is_ok())
		{
			if create_dds(&first_data).context("Converting first TEXT to DDS")?
				!= create_dds(&second_data).context("Converting second TEXT to DDS")?
			{
				return Ok(ResourceDiff::Different {
					description: Some("Textures are different".into())
				});
			} else {
				return Ok(ResourceDiff::Same);
			}
		}

		if first_meta.scrambled != second_meta.scrambled || first_meta.references != second_meta.references {
			return Ok(ResourceDiff::Different {
				description: Some("Metadata is different".into())
			});
		}

		if first_data.pack_to_vec()? != second_data.pack_to_vec()? {
			ResourceDiff::Different {
				description: Some("Texture data is different".into())
			}
		} else {
			ResourceDiff::Same
		}
	}

	#[try_fn]
	fn extract(&self, meta: &ResourceMetadata, data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
		vec![(
			format!("{}.{}.dds", meta.id.to_hash(), meta.resource_type),
			create_dds(&TextureMap::from_memory(data, WoaVersion::HM3)?)?
		)]
	}
}

pub struct TEXDDiff;

inventory::submit!(&TEXDDiff as &dyn DiffProvider);

impl DiffProvider for TEXDDiff {
	fn resource_type(&self) -> &'static str {
		"TEXD"
	}

	#[try_fn]
	fn diff(
		&self,
		_: &ResourceMetadata,
		first_data: &[u8],
		_: &ResourceMetadata,
		second_data: &[u8]
	) -> Result<ResourceDiff> {
		let first_data = MipblockData::from_memory(first_data, WoaVersion::HM3).context("Parsing first TEXD")?;
		let second_data = MipblockData::from_memory(second_data, WoaVersion::HM3).context("Parsing second TEXD")?;

		if first_data.pack_to_vec(WoaVersion::HM3)? != second_data.pack_to_vec(WoaVersion::HM3)? {
			ResourceDiff::Different {
				description: Some("Mipblock data is different".into())
			}
		} else {
			ResourceDiff::Same
		}
	}
}
