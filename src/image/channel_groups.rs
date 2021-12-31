
use std::collections::HashMap;
use crate::image::write::channels::{WritableChannels, ChannelsWriter};
use crate::meta::attribute::{LevelMode, ChannelList, Text, TextSlice, ChannelInfo};
use crate::meta::header::Header;
use crate::image::read::layers::{ReadChannels, ChannelsReader};
use crate::block::{BlockIndex, UncompressedBlock};
use crate::block::lines::{collect_uncompressed_block_from_lines, LineIndex};
use std::io::{Cursor, Read};
use crate::error::{Result, UnitResult};
use crate::block::chunk::TileCoordinates;
use crate::prelude::SmallVec;
use smallvec::alloc::collections::{BTreeSet, BTreeMap};
use std::iter::FromIterator;


#[derive(Default, Eq, PartialEq, Debug)]
pub struct Groups<Channels> {
    own_channels: Option<Channels>,
    child_groups: BTreeMap<Text, Self>
}


impl<Channels> Groups<Channels>  {


    // pub fn visit_groups_mut(&mut self, visitor: impl Fn(&mut Channels)) {
    // }



    // pub fn all_channels(&self) -> SmallVec<[&Channels; 12]> {
    //     let children = self.child_groups.iter().flat_map(|group| group.groups());
    //     self.channels.iter().chain(children).collect()
    // }

    pub fn all_channel_groups(&self) -> impl Iterator<Item=&Channels> {
        let children = self.child_groups.iter()
            .flat_map(|group| group.all_channel_groups());

        self.own_channels.iter().chain(children)
    }

    // pub fn absolute_channels(&self) -> impl Iterator<Item=Channels> {
    //     let children = self.child_groups.iter()
    //         .flat_map(|(name, child)|
    //             child.absolute_channels().map(|channel: Channels| {
    //                 let mut channel = channel.clone();
    //             })
    //         );
    //
    //     self.own_channels.iter().chain(children)//.collect()
    // }

    pub fn lookup_channel_group(&self, group_name: &TextSlice) -> Option<&Channels> {
        let dot_index = group_name.iter().position('.');
        if let Some(dot_index) = dot_index {
            let group_name = &group_name[.. dot_index];
            let child_name = &group_name[dot_index + 1 ..];
            self.child_groups.get(group_name)
                .and_then(|child| child.lookup(child_name))
        }
        else {
            self.own_channels.lookup(name)
        }
    }


    /*pub fn insert_group(&mut self, full_name: &TextSlice, value: ChannelGroup) {
        let dot_index = full_name.iter().position('.');
        if let Some(dot_index) = dot_index {
            let group_name = &group_name[.. dot_index];
            let name_rest = &group_name[dot_index + 1 ..];

            self.children.entry(Text::from_slice_unchecked(group_name))
                .or_insert(|| );

            // self.children.insert(Text::from_slice_unchecked(group_name), value)
            //     .and_then(|child| child.lookup(name_rest));
        }
        else {
            self.channel_group.lookup(name);
        }
    }*/

    fn map<T>(self, mapper: impl FnMut(Channels) -> T) -> Groups<T> {
        Groups {
            own_channels: self.own_channels.map(&mapper),
            child_groups: self.child_groups.into_iter()
                .map(|(name, child)| (name, child.map(&mapper)))
                .collect(),
        }
    }

    fn try_map<T>(self, mapper: impl FnMut(Channels) -> Result<T>) -> Result<Groups<T>> {
        Ok(Groups {
            own_channels: self.own_channels.map(&mapper)?,
            child_groups: self.child_groups.into_iter()
                .map(|(name, child)| Ok((name, child.try_map(&mapper)?)))
                .into()?.collect(),
        })
    }
}

type SmallIndicesVec = SmallVec<[usize; 12]>;

impl Groups<SmallIndicesVec> {

    // returns indices that reference the argument items
    pub fn parse_list_to_indices(channels: impl Iterator<Item=Text>) -> Self {
        let mut result = Groups::default();

        for (index, name) in channels.enumerate() {
            insert_into_groups(&mut result, name, index);
        }

        result
    }

    fn insert_into_groups(&mut self, name: Text, item_index: usize) {
        let dot_index = name.as_slice().iter().position('.');

        if let Some(dot_index) = dot_index {
            // insert into child group

            let group_name = Text::from_slice_unchecked(&name.as_slice()[.. dot_index]);
            let child_channel = Text::from_slice_unchecked(&name.as_slice()[dot_index + 1 ..]);

            let child_group = self.child_groups.entry(group_name)
                .or_insert_with(Groups::default);

            child_group.insert_into_groups(child_channel, item_index);
        }

        else {
            // insert directly into group
            let groups = self.own_channels.get_or_insert_with(SmallIndicesVec::new);
            groups.push(item_index);
        }
    }
}


impl<'slf, Channels> Groups<Channels> where Channels: WritableChannels<'slf> {
    pub fn into_absolute_names(self) -> impl Iterator<Item=(Text, Channels)> {
        let child_channels = self.child_groups.iter().flat_map(|(child_name, child)| {
            child.into_absolute_names().map(|(mut name, value)| {
                name.push_front(child_name);
                (name, value)
            });
        });

        let own_channels = self.own_channels
            // TODO check empty and throw?
            .map(|own| own.infer_channel_list().list)
            .flatten();

        child_channels.chain(own_channels)
    }
}

impl<'slf, ChannelGroup> WritableChannels<'slf> for Groups<ChannelGroup>
    where ChannelGroup: WritableChannels<'slf>
{
    fn infer_channel_list(&self) -> ChannelList {
        let mut all_channels = self.into_absolute_names().collect();
        all_channels.sort_unstable(); // TODO check empty and throw?
        ChannelList::new(all_channels) // might be empty, but will be checked in MetaData::validate()
    }

    fn infer_level_modes(&self) -> LevelMode {
        fn find_mode_or_none(channels: &Self) -> Option<LevelMode> {
            channels.own_channels.map(WritableChannels::level_mode).or_else(|| {
                channels.child_groups.iter().map(find_mode_or_none).next()
            })
        }

        let mode = find_mode_or_none(self)
            .expect("empty channel groups (check failed)"); // TODO only happens for empty channels, right? panic maybe?

        if let Some(chans) = self.own_channels.as_ref() {
            debug_assert_eq!(chans.level_mode(), mode, "level mode must be equal for all legacy channel groups")
        }

        debug_assert!(
            self.child_groups.values()
                .flat_map(find_mode_or_none)
                .all(|child_mode| child_mode == mode),

            "level mode must be equal for all legacy channel groups"
        );

        mode
    }

    type Writer = GroupChannelsWriter<'slf, ChannelGroup>;

    fn create_writer(&'slf self, header: &Header) -> Self::Writer {
        GroupChannelsWriter {
            all_channel_groups: self.all_channel_groups()
                .map(|channel_group: ChannelGroup| channel_group.create_writer(header))
                .collect()
        }
    }
}

struct GroupChannelsWriter<'c, ChannelGroupWriter> {
    all_channel_groups: Vec<&'c ChannelGroupWriter>,
}

impl<'c, Channels> ChannelsWriter for GroupChannelsWriter<'c, Channels> where Channels: ChannelsWriter {
    fn extract_uncompressed_block(&self, header: &Header, block_index: BlockIndex, output_block_data: &mut [u8]) {
        for channels_group in self.all_channel_groups {
            channels_group.extract_uncompressed_block(header, block_index, output_block_data);
        }
    }
}


struct ReadChannelGroups<ReadChannelGroup> {
    read_channels: ReadChannelGroup
}

struct ChannelGroupsReader<ChannelGroupReader> {
    channels: Groups<ChannelGroupReader>,

    // TODO optimize by iterating a vec instead of the nested groups:
    //channels: Groups<usize>,
    //indexed_channels: Vec<ChannelGroupReader>,
}

impl<'s, ReadChannelGroup> ReadChannels<'s> for ReadChannelGroups<ReadChannelGroup>
    where ReadChannelGroup: ReadChannels<'s>
{
    type Reader = ChannelGroupsReader<ReadChannelGroup::Reader>;

    fn create_channels_reader(&'s self, header: &Header, selected_channels_indices: &[usize]) -> Result<Self::Reader> {
        // indices refer to `selected_channels_indices`
        let channel_groups = parse_list_to_indices(
            selected_channels_indices.iter()
                .map(|&index| header.channels.list[index])
                .map(|selected_channel| selected_channel.name)
        );

        Ok(ChannelGroupsReader {
            // own_channels_indices refer to `selected_channels_indices`
            channels: channel_groups.try_map(&|group_own_channel_indices|{

                let group_selected_channel_indices = group_own_channel_indices.iter()
                    .map(|index| selected_channels_indices[index])
                    .collect::<SmallIndicesVec>();

                let reader = self.read_channels.create_channels_reader(header, &group_selected_channel_indices);
                reader
            })?
        })
    }
}

impl<ChannelGroupReader> ChannelsReader for ChannelGroupsReader<ChannelGroupReader> where ChannelGroupReader: ChannelsReader {
    type Channels = Groups<ChannelGroupReader::Channels>;

    fn is_block_desired(&self, tile: (usize, &TileCoordinates)) -> bool {
        self.channels.all_channel_groups().any(|channel_group| channel_group.filter_block(tile))
    }

    // for every incoming block, all the children read the lines they want into their temporary storage
    fn read_block(&mut self, header: &Header, block: &UncompressedBlock) -> UnitResult {
        for channel in self.channels.all_channel_groups() {
            channel.read_block(header, block)?;
        }

        Ok(())
    }

    fn into_channels(self) -> Self::Channels {
        self.channels.map(|channel_group_reader| channel_group_reader.into_channels())
    }
}