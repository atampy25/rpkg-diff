use anyhow::Result;
use ecow::EcoString;
use glacier_commons::metadata::ResourceMetadata;

pub mod entity;
pub mod json;
pub mod material;
pub mod texture;

#[derive(Default, PartialEq, Eq, Clone)]
pub enum ResourceDiff {
	OnlyInFirst {
		is_vanilla: bool
	},

	OnlyInSecond {
		is_vanilla: bool
	},

	Different {
		description: Option<EcoString>
	},

	#[default]
	Same
}

impl PartialOrd for ResourceDiff {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for ResourceDiff {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		let rank = |x: &ResourceDiff| match x {
			ResourceDiff::OnlyInFirst { is_vanilla } => {
				if *is_vanilla {
					3u8
				} else {
					0
				}
			}

			ResourceDiff::OnlyInSecond { is_vanilla } => {
				if *is_vanilla {
					4u8
				} else {
					1
				}
			}

			ResourceDiff::Different { .. } => 2,

			ResourceDiff::Same => 5
		};

		rank(self).cmp(&rank(other))
	}
}

pub trait DiffProvider: Send + Sync {
	/// The resource type this provider can handle.
	fn resource_type(&self) -> &'static str;

	/// Compare two resources of this type and return a description of the differences if any.
	fn diff(
		&self,
		first_meta: &ResourceMetadata,
		first_data: &[u8],
		second_meta: &ResourceMetadata,
		second_data: &[u8]
	) -> Result<ResourceDiff>;

	/// Extract the resource into human-readable files for easier investigation.
	#[allow(unused_variables)]
	fn extract(&self, meta: &ResourceMetadata, data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
		Ok(vec![])
	}
}

inventory::collect!(&'static dyn DiffProvider);

pub fn diff(
	first_meta: &ResourceMetadata,
	first_data: &[u8],
	second_meta: &ResourceMetadata,
	second_data: &[u8]
) -> Result<ResourceDiff> {
	for provider in inventory::iter::<&dyn DiffProvider> {
		if first_meta.resource_type == provider.resource_type() {
			return provider.diff(first_meta, first_data, second_meta, second_data);
		}
	}

	if first_data != second_data {
		Ok(ResourceDiff::Different { description: None })
	} else if first_meta.resource_type != second_meta.resource_type
		|| first_meta.scrambled != second_meta.scrambled
		|| first_meta.references != second_meta.references
	{
		Ok(ResourceDiff::Different {
			description: Some("Data is same but metadata is different".into())
		})
	} else {
		Ok(ResourceDiff::Same)
	}
}
