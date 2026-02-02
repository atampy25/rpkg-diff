use std::{
	env,
	fs::{self, File},
	path::{Path, PathBuf},
	process,
	sync::Arc
};

use anyhow::{Context as _, Result, bail};
use clap::{CommandFactory, Parser};
use clap_complete::Shell;
use compress_tools::{ArchiveContents, ArchiveIteratorBuilder};
use fn_error_context::context;
use hitman_commons::{
	hash_list::HASH_LIST,
	metadata::{ResourceMetadata, RuntimeID},
	rpkg_tool::RpkgResourceMeta
};
use iocraft::prelude::*;
use itertools::{EitherOrBoth, Itertools};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use rpkg_rs::resource::{
	partition_manager::PartitionManager, pdefs::PackageDefinitionSource, resource_info::ResourceInfo,
	resource_package::ResourcePackage, runtime_resource_id::RuntimeResourceID
};
use tryvial::try_fn;

use rpkg_diff::{DiffProvider, ResourceDiff, diff};

/// Diff two RPKG files with regard to only the important (semantic) differences.
#[derive(Debug, clap::Parser)]
#[command(
	version,
	about,
	arg_required_else_help = true,
	args_conflicts_with_subcommands = true
)]
pub struct Args {
	#[command(subcommand)]
	pub subcommand: Option<Subcommand>,

	#[command(flatten)]
	pub args: Option<DiffArgs>
}

#[derive(Debug, clap::Parser)]
pub enum Subcommand {
	GenerateCompletions {
		/// The shell to generate completions for.
		shell: Shell,

		/// The output directory for the completions.
		out: PathBuf
	}
}

#[derive(Debug, clap::Parser)]
pub struct DiffArgs {
	/// Path to first RPKG to compare (or an archive file containing it).
	pub first: PathBuf,

	/// Path filter for first RPKG, if inside an archive file.
	#[arg(visible_alias = "f1", long)]
	pub first_filter: Option<String>,

	/// Path to second RPKG to compare (or an archive file containing it).
	pub second: PathBuf,

	/// Path filter for second RPKG, if inside an archive file.
	#[arg(visible_alias = "f2", long)]
	pub second_filter: Option<String>,

	/// Compare the RPKG files as base packages (e.g. chunk0) rather than patches (e.g. chunk0patch5).
	#[arg(short, long, default_value_t = false)]
	pub base: bool,

	/// Path to vanilla game Runtime folder to detect resources that are identical to vanilla.
	#[arg(short, long)]
	pub vanilla_runtime: Option<PathBuf>
}

#[try_fn]
#[tokio::main]
async fn main() -> Result<()> {
	let args = Args::parse();

	if let Some(subcommand) = args.subcommand {
		match subcommand {
			Subcommand::GenerateCompletions { shell, out } => {
				fs::create_dir_all(&out).context("Creating output directory")?;
				clap_complete::generate_to(shell, &mut Args::command(), "rpkg-diff", out)
					.context("Generating bash completions")?;
			}
		}

		return Ok(());
	}

	let Some(args) = args.args else {
		Args::command().print_long_help()?;
		process::exit(exitcode::USAGE);
	};

	let hash_list_path = env::current_exe()?.with_file_name("hash_list.sml");
	if hash_list_path.exists() {
		eprintln!("Loading hash list");
		HASH_LIST
			.load_compressed(&fs::read(hash_list_path).context("Reading hash list")?)
			.context("Loading hash list")?;
	} else {
		eprintln!(
			"No hash list available, download and place it next to the executable as hash_list.sml to see paths/hints"
		);
	}

	eprintln!("Parsing first RPKG file from {}", args.first.display());
	let (first_name, first) = read_rpkg(&args.first, args.first_filter).context("Reading first RPKG")?;
	let first = Arc::new(ResourcePackage::from_memory(first, !args.base).context("Parsing first RPKG")?);

	eprintln!("Parsing second RPKG file from {}", args.second.display());
	let (second_name, second) = read_rpkg(&args.second, args.second_filter).context("Reading second RPKG")?;
	let second = Arc::new(ResourcePackage::from_memory(second, !args.base).context("Parsing second RPKG")?);

	let vanilla_files = if let Some(vanilla_runtime) = args.vanilla_runtime {
		eprintln!("Loading vanilla runtime from {}", vanilla_runtime.display());
		let mut partitions = PackageDefinitionSource::HM3(
			fs::read(vanilla_runtime.join("packagedefinition.txt")).context("Reading packagedefinition.txt")?
		)
		.read()
		.context("Parsing packagedefinition")?;

		for partition in &mut partitions {
			partition.set_max_patch_level(9);
		}

		let mut partition_manager =
			PartitionManager::new(vanilla_runtime, &PackageDefinitionSource::Custom(partitions))
				.context("Creating partition manager")?;

		partition_manager
			.mount_partitions(|_, _| {})
			.context("Mounting partitions")?;

		Some(partition_manager)
	} else {
		None
	};

	element! {
		ContextProvider(value: Context::owned(AppContext::default())) {
			MainView(first_name, first, second_name, second, vanilla_files)
		}
	}
	.fullscreen()
	.await?;
}

#[try_fn]
#[context("Reading RPKG file")]
fn read_rpkg(path: &Path, filter: Option<String>) -> Result<(String, Vec<u8>)> {
	if path.to_string_lossy().ends_with(".rpkg") {
		(
			path.to_string_lossy().into(),
			fs::read(path).context("Reading file on disk")?
		)
	} else {
		let filter = Arc::new(filter);
		let mut archive_iter = ArchiveIteratorBuilder::new(File::open(path).context("Opening file on disk")?)
			.filter(move |entry, _| entry.ends_with(".rpkg") && (*filter).as_ref().is_none_or(|f| entry.contains(f)))
			.build()
			.context("Opening archive from file")?
			.skip_while(|x| !matches!(x, ArchiveContents::StartOfEntry(_, _)));

		let Some(ArchiveContents::StartOfEntry(entry, _)) = archive_iter.next() else {
			unreachable!()
		};

		let chunks = archive_iter
			.map_while(|x| {
				if let ArchiveContents::DataChunk(data) = x {
					Some(data)
				} else {
					None
				}
			})
			.collect::<Vec<_>>();

		if chunks.is_empty() {
			bail!("No .rpkg file found in archive");
		}

		(format!("{}/{}", path.to_string_lossy(), entry), chunks.concat())
	}
}

#[try_fn]
#[context("Comparing resources {:?}", resources.as_ref().map_any(|x| x.rrid(), |x| x.rrid()))]
fn diff_resources(
	vanilla_files: Option<&PartitionManager>,
	first_rpkg: &ResourcePackage,
	second_rpkg: &ResourcePackage,
	resources: EitherOrBoth<&ResourceInfo, &ResourceInfo>
) -> Result<ResourceDiff> {
	let is_vanilla = |rpkg: &ResourcePackage, res: &ResourceInfo| {
		if let Some(vanilla_files) = vanilla_files {
			if let Some(p) = vanilla_files.partitions_with_resource(res.rrid()).first().cloned() {
				let partition = vanilla_files.find_partition(p).context("No such partition")?;

				let first_meta = ResourceMetadata::try_from(
					partition
						.get_resource_info(res.rrid())
						.context("Getting resource info from vanilla partition")?
				)?;

				let first_data = partition
					.read_resource(res.rrid())
					.context("Reading resource from vanilla partition")?;

				let second_meta = ResourceMetadata::try_from(res)?;
				let second_data = rpkg.read_resource(res.rrid())?;

				anyhow::Ok(diff(&first_meta, &first_data, &second_meta, &second_data)? == ResourceDiff::Same)
			} else {
				Ok(false)
			}
		} else {
			Ok(false)
		}
	};

	match resources {
		EitherOrBoth::Both(first, second) => {
			let first_meta = ResourceMetadata::try_from(first)?;
			let first_data = first_rpkg.read_resource(first.rrid())?;
			let second_meta = ResourceMetadata::try_from(second)?;
			let second_data = second_rpkg.read_resource(second.rrid())?;

			diff(&first_meta, &first_data, &second_meta, &second_data)?
		}

		EitherOrBoth::Left(res) => ResourceDiff::OnlyInFirst {
			is_vanilla: is_vanilla(first_rpkg, res)?
		},

		EitherOrBoth::Right(res) => ResourceDiff::OnlyInSecond {
			is_vanilla: is_vanilla(second_rpkg, res)?
		}
	}
}

#[derive(Default, Props)]
pub struct MainProps {
	first_name: String,
	first: Option<Arc<ResourcePackage>>,

	second_name: String,
	second: Option<Arc<ResourcePackage>>,

	vanilla_files: Option<PartitionManager>
}

#[derive(Default)]
struct AppContext {
	cached_diffs: Arc<papaya::HashMap<RuntimeResourceID, ResourceDiff>>
}

#[try_fn]
#[context("Couldn't extract resource {}.{}.{}", resource.rrid(), kind, resource.data_type())]
fn extract_resource(rpkg: &ResourcePackage, resource: &ResourceInfo, kind: &str) -> Result<()> {
	let output_dir = Path::new("extracted");
	fs::create_dir_all(output_dir)?;

	let data = rpkg.read_resource(resource.rrid())?;
	fs::write(
		output_dir.join(format!("{}_{}.{}", kind, resource.rrid(), resource.data_type())),
		&data
	)?;

	fs::write(
		output_dir.join(format!(
			"{}_{}.{}.metadata.json",
			kind,
			resource.rrid(),
			resource.data_type()
		)),
		serde_json::to_vec_pretty(&ResourceMetadata::try_from(resource)?)?
	)?;

	fs::write(
		output_dir.join(format!(
			"{}_{}.{}.meta.JSON",
			kind,
			resource.rrid(),
			resource.data_type()
		)),
		serde_json::to_vec_pretty(&RpkgResourceMeta::try_from(resource)?)?
	)?;

	for provider in inventory::iter::<&dyn DiffProvider> {
		if resource.data_type() == provider.resource_type() {
			for (filename, data) in provider.extract(&ResourceMetadata::try_from(resource)?, &data)? {
				fs::write(output_dir.join(format!("{kind}_{filename}")), data)?;
			}

			break;
		}
	}
}

#[component]
fn MainView<'a>(props: &'a MainProps, mut hooks: Hooks) -> impl Into<AnyElement<'a>> {
	let (width, height) = hooks.use_terminal_size();
	let mut scroll_offset = hooks.use_state(|| 0usize);

	let context = hooks.use_context::<AppContext>();

	let first = props.first.as_ref().unwrap();
	let second = props.second.as_ref().unwrap();
	let vanilla_files = props.vanilla_files.as_ref();

	let cached_diffs = context.cached_diffs.clone();

	let resources = second
		.resources()
		.par_iter()
		.filter(|(x, _)| !first.resources().contains_key(*x))
		.map(EitherOrBoth::Right)
		.chain(first.resources().par_iter().map(|x| {
			if let Some(y) = second.resources().get(x.0) {
				EitherOrBoth::Both(x, (x.0, y))
			} else {
				EitherOrBoth::Left(x)
			}
		}))
		.map(|resources| {
			let id = resources.as_ref().left().or(resources.as_ref().right()).unwrap().0;

			(
				id,
				cached_diffs
					.pin()
					.get_or_insert_with(*id, || {
						diff_resources(vanilla_files, first, second, resources.map_any(|x| x.1, |x| x.1)).unwrap()
					})
					.to_owned()
			)
		})
		.collect::<Vec<_>>()
		.into_iter()
		.sorted_by(|a, b| a.1.cmp(&b.1).then(u64::from(*a.0).cmp(&u64::from(*b.0))))
		.collect_vec();

	let resources_count = resources.len();
	let resource_ids = resources.iter().map(|(id, _)| **id).collect_vec();

	let mut system = hooks.use_context_mut::<SystemContext>();
	let mut should_exit = hooks.use_state(|| false);

	hooks.use_terminal_events({
		let first = first.clone();
		let second = second.clone();
		move |event| match event {
			TerminalEvent::FullscreenMouse(FullscreenMouseEvent {
				kind: MouseEventKind::ScrollUp,
				..
			}) => {
				scroll_offset.set(scroll_offset.get().saturating_sub(1));
			}

			TerminalEvent::FullscreenMouse(FullscreenMouseEvent {
				kind: MouseEventKind::ScrollDown,
				..
			}) => {
				scroll_offset.set(scroll_offset.get().saturating_add(1).min(resources_count - 1));
			}

			TerminalEvent::Key(KeyEvent { code, kind, .. }) if kind != KeyEventKind::Release => match code {
				KeyCode::Up => scroll_offset.set(scroll_offset.get().saturating_sub(1)),
				KeyCode::PageUp => scroll_offset.set(scroll_offset.get().saturating_sub(50)),
				KeyCode::Down => scroll_offset.set(scroll_offset.get().saturating_add(1).min(resources_count - 1)),
				KeyCode::PageDown => scroll_offset.set(scroll_offset.get().saturating_add(50).min(resources_count - 1)),

				KeyCode::Char('q') | KeyCode::Char('Q') => {
					should_exit.set(true);
				}

				KeyCode::Char('e') | KeyCode::Char('E') => {
					let resource = resource_ids.get(scroll_offset.get()).unwrap();

					if let Some(first_res) = first.resources().get(resource) {
						extract_resource(&first, first_res, "first").unwrap();
					}

					if let Some(second_res) = second.resources().get(resource) {
						extract_resource(&second, second_res, "second").unwrap();
					}
				}

				KeyCode::Char('1') => {
					let output_dir = Path::new("extracted");
					fs::create_dir_all(output_dir).unwrap();

					for resource in resource_ids.iter().filter(|&id| {
						matches!(
							cached_diffs.pin().get(id).map(|x| x.to_owned()),
							Some(ResourceDiff::OnlyInFirst { .. })
						)
					}) {
						if let Some(first_res) = first.resources().get(resource) {
							extract_resource(&first, first_res, "first").unwrap();
						}
					}
				}

				KeyCode::Char('2') => {
					let output_dir = Path::new("extracted");
					fs::create_dir_all(output_dir).unwrap();

					for resource in resource_ids.iter().filter(|&id| {
						matches!(
							cached_diffs.pin().get(id).map(|x| x.to_owned()),
							Some(ResourceDiff::OnlyInSecond { .. })
						)
					}) {
						if let Some(second_res) = second.resources().get(resource) {
							extract_resource(&second, second_res, "second").unwrap();
						}
					}
				}

				KeyCode::Char('3') => {
					let output_dir = Path::new("extracted");
					fs::create_dir_all(output_dir).unwrap();

					for resource in resource_ids.iter().filter(|&id| {
						matches!(
							cached_diffs.pin().get(id).map(|x| x.to_owned()),
							Some(ResourceDiff::Different { .. })
						)
					}) {
						if let Some(first_res) = first.resources().get(resource) {
							extract_resource(&first, first_res, "first").unwrap();
						}

						if let Some(second_res) = second.resources().get(resource) {
							extract_resource(&second, second_res, "second").unwrap();
						}
					}
				}

				_ => {}
			},
			_ => {}
		}
	});

	if should_exit.get() {
		system.exit();
	}

	let resource_rows = resources
		.iter()
		.enumerate()
		.skip(scroll_offset.get())
		.take(height as usize)
		.map(|(idx, (id, diff))| {
			element!(DiffRow(
				key: idx,
				idx,
				id: **id,
				diff: diff.to_owned(),
				first: props.first.as_deref(),
				second: props.second.as_deref(),
				selected: scroll_offset.get() == idx,
				clicked: move |_| {
					scroll_offset.set(idx);
				}
			))
		});

	element! {
		View(width, height, flex_direction: FlexDirection::Column, gap: 1) {
			View(flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap) {
				Text(content: "There are ")
				Text(
					color: Color::Blue,
					weight: Weight::Bold,
					content: props.first.as_ref().unwrap().resources().len().to_string()
				)
				Text(content: " resources in the first RPKG file and ")
				Text(
					color: Color::Blue,
					weight: Weight::Bold,
					content: props.second.as_ref().unwrap().resources().len().to_string()
				)
				Text(content: " resources in the second RPKG file. ")
				Text(
					color: Color::Red,
					weight: Weight::Bold,
					content: resources
						.iter()
						.filter(|x| matches!(x.1, ResourceDiff::OnlyInFirst { .. }))
						.count()
						.to_string()
				)
				#(if resources.iter().any(|x| matches!(x.1, ResourceDiff::OnlyInFirst { is_vanilla: true })) {
					Some(element!(View {
						Text(content: " (")
						Text(
							color: Color::Blue,
							weight: Weight::Bold,
							content: resources
								.iter()
								.filter(|x| matches!(x.1, ResourceDiff::OnlyInFirst { is_vanilla: true }))
								.count()
								.to_string()
						)
						Text(content: " same as vanilla)")
					}))
				} else {
					None
				})
				Text(content: " resources are only in the first RPKG file, ")
				Text(
					color: Color::Red,
					weight: Weight::Bold,
					content: resources
						.iter()
						.filter(|x| matches!(x.1, ResourceDiff::OnlyInSecond { .. }))
						.count()
						.to_string()
				)
				#(if resources.iter().any(|x| matches!(x.1, ResourceDiff::OnlyInSecond { is_vanilla: true })) {
					Some(element!(View {
						Text(content: " (")
						Text(
							color: Color::Blue,
							weight: Weight::Bold,
							content: resources
								.iter()
								.filter(|x| matches!(x.1, ResourceDiff::OnlyInSecond { is_vanilla: true }))
								.count()
								.to_string()
						)
						Text(content: " same as vanilla)")
					}))
				} else {
					None
				})
				Text(content: " are only in the second RPKG file, ")
				Text(
					color: Color::Yellow,
					weight: Weight::Bold,
					content: resources
						.iter()
						.filter(|x| matches!(x.1, ResourceDiff::Different { .. }))
						.count()
						.to_string()
				)
				Text(content: " are different between the two and ")
				Text(
					color: Color::Green,
					weight: Weight::Bold,
					content: resources.iter().filter(|x| x.1 == ResourceDiff::Same).count().to_string()
				)
				Text(content: " are the same.")
			}

			View(
				flex_direction: FlexDirection::Column,
				border_style: BorderStyle::Round,
				border_color: Color::Cyan,
				overflow: Overflow::Hidden,
				height: 100pct
			) {
				View(
					width: 100pct,
					border_style: BorderStyle::Round,
					border_edges: Edges::Bottom,
					border_color: Color::DarkGrey
				) {
					View(
						width: 5pct,
						border_style: BorderStyle::Round,
						border_edges: Edges::Right,
						border_color: Color::DarkGrey,
						justify_content: JustifyContent::Center
					) {
						Text(content: " #")
					}

					View(
						width: 20pct,
						border_style: BorderStyle::Round,
						border_edges: Edges::Right,
						border_color: Color::DarkGrey,
						justify_content: JustifyContent::Center
					) {
						Text(content: "Resource")
					}

					View(
						width: 5pct,
						border_style: BorderStyle::Round,
						border_edges: Edges::Right,
						border_color: Color::DarkGrey,
						justify_content: JustifyContent::Center
					) {
						Text(content: "Type")
					}

					View(
						width: 35pct,
						border_style: BorderStyle::Round,
						border_edges: Edges::Right,
						border_color: Color::DarkGrey,
						justify_content: JustifyContent::Center
					) {
						Text(content: &props.first_name)
					}

					View(width: 35pct, justify_content: JustifyContent::Center) {
						Text(content: &props.second_name)
					}
				}

				#(resource_rows)
			}

			View(flex_direction: FlexDirection::Row, gap: 2) {
				View(flex_direction: FlexDirection::Row, gap: 1) {
					Text(content: "Up/Down", color: Color::Blue)
					Text(content: "Scroll")
				}

				View(flex_direction: FlexDirection::Row, gap: 1) {
					Text(content: "PgUp/PgDown", color: Color::Blue)
					Text(content: "Scroll faster")
				}

				View(flex_direction: FlexDirection::Row, gap: 1) {
					Text(content: "E", color: Color::Blue)
					Text(content: "Extract selected resource")
				}

				View(flex_direction: FlexDirection::Row, gap: 1) {
					Text(content: "1", color: Color::Blue)
					Text(content: "Extract resources only present in first")
				}

				View(flex_direction: FlexDirection::Row, gap: 1) {
					Text(content: "2", color: Color::Blue)
					Text(content: "Extract resources only present in second")
				}

				View(flex_direction: FlexDirection::Row, gap: 1) {
					Text(content: "3", color: Color::Blue)
					Text(content: "Extract resources with differences")
				}
			}
		}
	}
}

#[derive(Default, Props)]
struct DiffRowProps<'a> {
	first: Option<&'a ResourcePackage>,
	second: Option<&'a ResourcePackage>,

	selected: bool,
	idx: usize,
	id: RuntimeResourceID,
	diff: ResourceDiff,

	clicked: HandlerMut<'static, ()>
}

#[component]
fn DiffRow<'a>(props: &mut DiffRowProps<'a>) -> impl Into<AnyElement<'static>> {
	let first = props.first.unwrap();
	let second = props.second.unwrap();

	let resources = match props.diff {
		ResourceDiff::Same | ResourceDiff::Different { .. } => EitherOrBoth::Both(
			(props.id, first.resources().get(&props.id).unwrap()),
			(props.id, second.resources().get(&props.id).unwrap())
		),

		ResourceDiff::OnlyInFirst { .. } => EitherOrBoth::Left((props.id, first.resources().get(&props.id).unwrap())),

		ResourceDiff::OnlyInSecond { .. } => EitherOrBoth::Right((props.id, second.resources().get(&props.id).unwrap()))
	};

	let diff_details = match &props.diff {
		ResourceDiff::Same => element!(View(width: 70pct, justify_content: JustifyContent::Center) {
			Text(content: "Resources are the same", color: Color::Green)
		}),

		ResourceDiff::Different { description } => {
			if let Some(description) = description {
				element!(
					View(width: 70pct, position: Position::Relative, justify_content: JustifyContent::Center) {
						View(width: 50pct, justify_content: JustifyContent::Center) {
							Text(
								content: resources.as_ref().left().unwrap().1.size().to_string(),
								color: Color::Yellow
							)

							Text(content: " bytes")
						}

						View(width: 50pct, justify_content: JustifyContent::Center) {
							Text(
								content: resources.as_ref().right().unwrap().1.size().to_string(),
								color: Color::Yellow
							)

							Text(content: " bytes")
						}

						View(width: 100pct, position: Position::Absolute, justify_content: JustifyContent::Center) {
							View(width: 30pct, justify_content: JustifyContent::Center) {
								Text(align: TextAlign::Center, content: description)
							}
						}
					}
				)
			} else {
				element!(
					View(width: 70pct, justify_content: JustifyContent::Center) {
						View(width: 50pct, justify_content: JustifyContent::Center) {
							Text(
								content: resources.as_ref().left().unwrap().1.size().to_string(),
								color: Color::Yellow
							)

							Text(content: " bytes")
						}

						View(width: 50pct, justify_content: JustifyContent::Center) {
							Text(
								content: resources.as_ref().right().unwrap().1.size().to_string(),
								color: Color::Yellow
							)

							Text(content: " bytes")
						}
					}
				)
			}
		}

		ResourceDiff::OnlyInFirst { is_vanilla } => element!(
			View(width: 70pct, justify_content: JustifyContent::Center) {
				#(if *is_vanilla {
					element!(
						View(width: 50pct, justify_content: JustifyContent::Center) {
							Text(content: "Same as vanilla (")
							Text(
								content: resources.as_ref().left().unwrap().1.size().to_string(),
								color: Color::Blue
							)
							Text(content: " bytes)")
						}
					)
				} else {
					element!(
						View(width: 50pct, justify_content: JustifyContent::Center) {
							Text(
								content: resources.as_ref().left().unwrap().1.size().to_string(),
								color: Color::Red
							)

							Text(content: " bytes")
						}
					)
				})

				View(width: 50pct, justify_content: JustifyContent::Center) {
					Text(content: "Not present", color: Color::DarkGrey)
				}
			}
		),

		ResourceDiff::OnlyInSecond { is_vanilla } => element!(
			View(width: 70pct, justify_content: JustifyContent::Center) {
				View(width: 50pct, justify_content: JustifyContent::Center) {
					Text(content: "Not present", color: Color::DarkGrey)
				}

				#(if *is_vanilla {
					element!(
						View(width: 50pct, justify_content: JustifyContent::Center) {
							Text(content: "Same as vanilla (")
							Text(
								content: resources.as_ref().right().unwrap().1.size().to_string(),
								color: Color::Blue
							)
							Text(content: " bytes)")
						}
					)
				} else {
					element!(
						View(width: 50pct, justify_content: JustifyContent::Center) {
							Text(
								content: resources.as_ref().right().unwrap().1.size().to_string(),
								color: Color::Red
							)

							Text(content: " bytes")
						}
					)
				})
			}
		)
	};

	element! {
			Button(handler: props.clicked.take()) {
				View(width: 100pct, background_color: if props.selected { Some(Color::DarkGrey) } else { None }) {
					View(width: 5pct, justify_content: JustifyContent::Center) {
						Text(content: (props.idx + 1).to_string())
					}

					View(width: 20pct, align_items: AlignItems::Center, flex_direction: FlexDirection::Column) {
						Text(content: props.id.to_string())

						#(HASH_LIST.entries.load().get(&RuntimeID::try_from(props.id).unwrap()).and_then(|entry| {
							entry.path.as_ref().or(entry.hint.as_ref()).map(|path_or_hint| {
								element!(
									Text(
										color: if props.selected { Color::Grey } else { Color::DarkGrey },
										align: TextAlign::Center,
										content: path_or_hint
									)
								)
							})
						}))
					}

					View(width: 5pct, justify_content: JustifyContent::Center) {
						#(match resources.as_ref() {
							EitherOrBoth::Both((_, x), (_, y)) => if x.data_type() == y.data_type() {
								element!(Text(content: x.data_type()))
							} else {
								element!(Text(
									color: Color::Red,
									content: format!("{} / {}", x.data_type(), y.data_type())
								))
							},

							EitherOrBoth::Left((_, x)) | EitherOrBoth::Right((_, x)) => {
								element!(Text(content: x.data_type().to_string()))
							}
						})
					}

					#(diff_details)
				}
			}
	}
}
