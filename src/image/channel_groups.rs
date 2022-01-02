
use std::collections::HashMap;
use crate::image::write::channels::{WritableChannels, ChannelsWriter};
use crate::meta::attribute::{LevelMode, ChannelList, Text, TextSlice, ChannelDescription};
use crate::meta::header::Header;
use crate::image::read::layers::{ReadChannels, ChannelsReader};
use crate::block::{BlockIndex, UncompressedBlock};
use crate::error::{Result, UnitResult};
use crate::block::chunk::TileCoordinates;
use crate::prelude::SmallVec;
use crate::math::RoundingMode;
use crate::image::read::image::ChannelMask;


#[derive(Default, Eq, PartialEq, Debug)]
pub struct Groups<Channels> {
    own_channels: Option<Channels>,
    child_groups: HashMap<Text, Self>
}


/*#[derive(Default, Debug)]
pub struct GroupedChannels<Channels> {
    indices: Groups<usize>,
    entries: Vec<Channels>
}

impl<Channels> GroupedChannels<Channels>  {

    pub fn all_channel_groups_sorted(&self) -> impl Iterator<Item=&Channels> {
        self.entries.iter()
    }

    pub fn lookup_channel_group(&self, group_name: &TextSlice) -> Option<&Channels> {
        self.indices.lookup_channel_group(group_name)
            .map(|index| self.entries[index])
    }

    fn map<T>(self, mapper: impl FnMut(Channels) -> T) -> Groups<T> {
        Groups {
            own_channels: self.own_channels.map(&mapper),
            child_groups: self.child_groups.into_iter()
                .map(|(name, child)| (name, child.map(&mapper)))
                .collect(),
        }
    }
}*/

impl<Channels> Groups<Channels>  {

    // pub fn insert(&mut self, parent_group_name: Text, channels: Channels){
    //
    // }

    // TODO other construction methods


    // TODO depth first or not?
    pub fn all_channel_groups(&self) -> impl Iterator<Item=&Channels> {
        // https://fasterthanli.me/articles/recursive-iterators-rust

        // TODO check empty and throw?
        //Box::new(
            self.child_groups.iter()
                .flat_map(|(_, child)| child.all_channel_groups())
                .chain(self.own_channels.iter())

                .collect::<SmallVec<[&Channels; 20]>>().into_iter()
        //)
    }

    pub fn lookup_channel_group(&self, group_name: &TextSlice) -> Option<&Channels> {
        let dot_index = group_name.position(|character| character == '.' as u8);

        if let Some(dot_index) = dot_index {
            let group_name = &group_name[.. dot_index];
            let child_name = &group_name[dot_index + 1 ..];
            self.child_groups.get(group_name)
                .and_then(|child| child.lookup_channel_group(child_name))
        }
        else { // arrived at last identifier
            Some(self)
        }
    }



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
        channels.enumerate().fold(
            Groups::default(),
            |mut groups, (index, name)|{
                groups.insert_channel_index(name, index);
                groups
            }
        )
    }

    fn insert_channel_index(&mut self, name: Text, item_index: usize) {
        let dot_index = name.as_slice().iter().position('.');

        if let Some(dot_index) = dot_index {
            // insert into child group

            let group_name = Text::from_slice_unchecked(&name.as_slice()[.. dot_index]);
            let child_channel = Text::from_slice_unchecked(&name.as_slice()[dot_index + 1 ..]);

            let child_group = self.child_groups.entry(group_name)
                .or_insert_with(Groups::default);

            child_group.insert_channel_index(child_channel, item_index);
        }

        else {
            // insert directly into group
            let groups = self.own_channels.get_or_insert_with(SmallIndicesVec::new);
            groups.push(item_index);
        }
    }
}


impl<'slf, Channels> Groups<Channels> where Channels: WritableChannels<'slf> {
    pub fn into_absolute_names<Channel>(self, to_channels: impl Fn(Channels) -> &[Channel]) -> SmallVec<[Channel;20]> {
        let mut child_channels = self.child_groups.into_iter().flat_map(|(child_name, child)| {
            // child.into_absolute_names(&to_channels).map(move |(mut name, value)| {
            //     name.push_front(child_name.as_slice());
            //     (name, value)
            // }).as_slice()
            let mut children = child.into_absolute_names(&to_channels);
            for (mut name, _) in &mut children { name.push_front(child_name.as_slice()); }
            children.as_slice()
        });

        let own_channels = self.own_channels.into_iter()
            // TODO check empty and throw?
            .flat_map(|own| to_channels(own));

        child_channels.chain(own_channels)
            .collect()
    }
}

impl<'slf, ChannelGroup> WritableChannels<'slf> for Groups<ChannelGroup>
    where ChannelGroup: WritableChannels<'slf>
{
    fn infer_channel_list(&self) -> ChannelList {
        let mut all_channels = self
            .into_absolute_names::<ChannelDescription, _>(|chans| chans.infer_channel_list().list)

            .map(|(name, channel)|{
                channel.name = name;
                channel
            })

            .collect();

        all_channels.sort(); // TODO check empty and throw?
        ChannelList::new(all_channels) // might be empty, but will be checked in MetaData::validate()
    }

    ///  Generate the file meta data of whether and how resolution levels should be stored in the file
    fn infer_level_modes(&self) -> (LevelMode, RoundingMode) {
        let mode = self.all_channel_groups().map(WritableChannels::infer_level_modes).next();

        // fn find_mode_or_none(channels: &Groups<ChannelGroup>) -> Option<LevelMode> {
        //     channels.own_channels.map(WritableChannels::level_mode).or_else(|| {
        //         channels.child_groups.iter().map(find_mode_or_none).next()
        //     })
        // }

        let mode = mode//find_mode_or_none(self)
            .expect("empty channel groups (check failed)"); // TODO only happens for empty channels, right? panic maybe?


        debug_assert!(
            self.all_channel_groups().map(WritableChannels::level_mode)
                .all(|child_mode| child_mode == mode),

            "level mode must be equal for all legacy channel groups"
        );

        mode
    }

    type Writer = GroupChannelsWriter<ChannelGroup>;

    fn create_writer(&'slf self, header: &Header) -> Self::Writer {
        GroupChannelsWriter {
            all_channel_groups: self.all_channel_groups()
                .map(|channel_group: &ChannelGroup| channel_group.create_writer(header))
                .collect()
        }
    }
}

struct GroupChannelsWriter<ChannelGroupWriter> {
    all_channel_groups: Vec<ChannelGroupWriter>,
}

impl<Channels> ChannelsWriter for GroupChannelsWriter<Channels> where Channels: ChannelsWriter {
    fn extract_uncompressed_block(&self, header: &Header, block_index: BlockIndex, output_block_data: &mut [u8]) {
        for channels_group in &self.all_channel_groups {
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

    fn create_channels_reader(&'s self, header: &Header, selected_channels_indices: &ChannelMask) -> Result<Self::Reader> {
        let selected_channels_indices = selected_channels_indices
            .selected_channel_indices().collect::<SmallVec<[usize; 20]>>();

        // indices refer to `selected_channels_indices`
        let channel_groups = Groups::parse_list_to_indices(
            selected_channels_indices.iter()
                .map(|&index| header.channels.list[index])
                .map(|selected_channel| selected_channel.name)
        );

        Ok(ChannelGroupsReader {
            // own_channels_indices refer to `selected_channels_indices`
            channels: channel_groups.try_map(|group_own_channel_indices|{

                let group_selected_channel_indices = group_own_channel_indices.iter()
                    .map(|&index| selected_channels_indices[index]);

                let group_selected_channel_indices = ChannelMask::only(group_selected_channel_indices);

                let reader = self.read_channels.create_channels_reader(header, &group_selected_channel_indices);
                reader
            })?
        })
    }
}

impl<ChannelGroupReader> ChannelsReader for ChannelGroupsReader<ChannelGroupReader> where ChannelGroupReader: ChannelsReader {
    type Channels = Groups<ChannelGroupReader::Channels>;

    fn is_block_desired(&self, tile: TileCoordinates) -> bool {
        // TODO linear memory iterator
        self.channels.all_channel_groups().any(|channel_group| channel_group.is_block_desired(tile))
    }

    // for every incoming block, all the children read the lines they want into their temporary storage
    fn read_block(&mut self, header: &Header, block: &UncompressedBlock) -> UnitResult {
        for channel in self.channels.all_channel_groups() { // TODO linear memory iterator
            channel.read_block(header, block)?;
        }

        Ok(())
    }

    fn into_channels(self) -> Self::Channels {
        self.channels.map(|channel_group_reader| channel_group_reader.into_channels())
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse(){

    }
}