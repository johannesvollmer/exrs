
use std::collections::HashMap;
use crate::image::write::channels::{WritableChannels, ChannelsWriter};
use crate::meta::attribute::{LevelMode, ChannelList, Text};
use crate::meta::header::Header;
use crate::image::read::layers::{ReadChannels, ChannelsReader};
use crate::block::{BlockIndex, UncompressedBlock};
use crate::error::{Result, UnitResult};
use crate::block::chunk::TileCoordinates;
use crate::prelude::{SmallVec, ChannelDescription};
use crate::math::RoundingMode;
use crate::image::read::image::ChannelMask;
use std::iter::FromIterator;
use crate::image::AnyChannels;

pub trait ReadGroupedChannels: Sized {
    fn grouped_channels(self) -> ReadChannelGroups<Self> {
        ReadChannelGroups { read_channels: self }
    }
}

impl<T> ReadGroupedChannels for T
    where T: ReadChannels {}


#[derive(Default, Eq, PartialEq, Debug)]
pub struct ChannelGroups<Channels> {
    pub channels: Option<Channels>,
    pub children: HashMap<Text, Self>
}


impl<Channels> ChannelGroups<Channels>  {

    // pub fn insert(&mut self, parent_group_name: Text, channels: Channels){
    //
    // }

    // TODO other construction methods
    pub fn from_list<Txt: Into<Text>>(named_groups: impl IntoIterator<Item=(Txt, Channels)>) -> Self {
        Self { channels: None, children: HashMap::from_iter(named_groups) }
    }

    // TODO depth first or not?
    pub fn all_channel_groups(&self) -> impl Iterator<Item=&Channels> {
        // TODO https://fasterthanli.me/articles/recursive-iterators-rust
        self.children.iter()
            .flat_map(|(_, child)| child.all_channel_groups())
            .chain(self.channels.iter())
            .collect::<SmallVec<[&Channels; 20]>>().into_iter()
    }

    // TODO depth first or not?
    pub fn all_channel_groups_mut(&mut self) -> impl Iterator<Item=&mut Channels> {
        // TODO https://fasterthanli.me/articles/recursive-iterators-rust
        self.children.iter_mut()
            .flat_map(|(_, child)| child.all_channel_groups_mut())
            .chain(self.channels.iter_mut())
            .collect::<SmallVec<[&mut Channels; 20]>>().into_iter()
    }

    /*TODO pub fn lookup_channel_group(&self, group_name: &TextSlice) -> Option<&Channels> {
        let dot_index = group_name.iter().position(|&character| character == '.' as u8);

        if let Some(dot_index) = dot_index {
            let group_name = &group_name[.. dot_index];
            let child_name = &group_name[dot_index + 1 ..];
            self.child_groups.get(group_name)
                .and_then(|child| child.lookup_channel_group(child_name))
        }
        else { // arrived at last identifier
            self.own_channels.as_ref()
        }
    }*/



    fn map<T>(self, mut mapper: impl FnMut(Channels) -> T) -> ChannelGroups<T> {
        ChannelGroups {
            channels: self.channels.map(&mut mapper),
            children: self.children.into_iter()
                .map(|(name, child)| (name, child.map(&mut mapper)))
                .collect(),
        }
    }

    fn try_map<T>(self, mut mapper: impl FnMut(Channels) -> Result<T>) -> Result<ChannelGroups<T>> {
        let channels = match self.channels {
            Some(channels) => Some(mapper(channels)?),
            None => None,
        };

        let new_child_groups = HashMap::with_capacity(self.children.len());
        let child_groups = self.children.into_iter()
            .map(|(name, child)| Ok((name, child.try_map(&mut mapper)?)))
            .try_fold(
                new_child_groups,
                |mut map: HashMap<Text, ChannelGroups<T>>, item: Result<(Text, ChannelGroups<T>)>| {
                    // TODO this is complicated!
                    item.map(move |(k,v)| {
                        map.insert(k,v);
                        map
                    })
                }
            )?;

        Ok(ChannelGroups { channels, children: child_groups, })
    }
}

type SmallIndicesVec = SmallVec<[usize; 12]>;

impl ChannelGroups<SmallIndicesVec> {

    // returns indices that reference the argument items
    pub fn parse_list_to_indices(channels: impl Iterator<Item=Text>) -> Self {
        channels.enumerate().fold(
            ChannelGroups::default(),
            |mut groups, (index, name)|{
                groups.insert_channel_index(name, index);
                groups
            }
        )
    }

    fn insert_channel_index(&mut self, name: Text, item_index: usize) {
        let dot_index = name.as_slice().iter().position(|&character| character == '.' as u8);

        if let Some(dot_index) = dot_index {
            // insert into child group

            let group_name = Text::from_slice_unchecked(&name.as_slice()[.. dot_index]);
            let child_channel = Text::from_slice_unchecked(&name.as_slice()[dot_index + 1 ..]);

            let child_group = self.children.entry(group_name)
                .or_insert_with(ChannelGroups::default);

            child_group.insert_channel_index(child_channel, item_index);
        }

        else {
            // insert directly into group
            let groups = self.channels.get_or_insert_with(SmallIndicesVec::new);
            groups.push(item_index);
        }
    }
}


impl<'slf, Channels> ChannelGroups<Channels> where Channels: WritableChannels<'slf> {
    // TODO reduce tuples and make simpler
    pub fn absolute_names_unsorted<Channel>(
        &self,
        to_channels: impl Fn(&Channels) -> SmallVec<[Channel;5]>,
        channel_name: impl Fn(&mut Channel) -> &mut Text,
    ) -> SmallVec<[Channel;5]> {
        let child_channels = self.children.iter().flat_map(|(child_group_name, child_group)| {
            let mut children = child_group.absolute_names_unsorted(&to_channels, &channel_name);

            for channel in &mut children {
                channel_name(channel).push_front(
                    child_group_name.as_slice().iter().cloned().chain("." as u8)
                );
            }

            children
        });

        let own_channels = self.channels.iter()
            // TODO check empty and throw?
            .flat_map(|own| to_channels(own));

        child_channels.chain(own_channels)
            .collect()
    }
}

impl<'slf, ChannelGroup> WritableChannels<'slf> for ChannelGroups<ChannelGroup>
    where ChannelGroup: WritableChannelGroup<'slf>
{
    fn infer_channel_list(&self) -> ChannelList {
        let mut all_channels: SmallVec<[ChannelDescription; 5]> = self
            .absolute_names_unsorted(
                |chans| chans.infer_channel_list().list.clone(),
                |channel| &mut channel.name
            )
            .collect();

        all_channels.sort_by_key(|chan| chan.name.clone()); // TODO borrow? // TODO check empty and throw?
        ChannelList::new(all_channels) // might be empty, but will be checked in MetaData::validate()
    }

    ///  Generate the file meta data of whether and how resolution levels should be stored in the file
    fn infer_level_modes(&self) -> (LevelMode, RoundingMode) {
        let mode = self.all_channel_groups().map(WritableChannels::infer_level_modes)
            .next().expect("empty channel groups (check failed)"); // TODO only happens for empty channels, right? panic maybe?

        debug_assert!(
            self.all_channel_groups().map(WritableChannels::infer_level_modes)
                .all(|child_mode| child_mode == mode),

            "level mode must be equal for all legacy channel groups"
        );

        mode
    }

    type Writer = GroupChannelsWriter<ChannelGroup::Writer>;

    fn create_writer(&'slf self, header: &Header) -> Self::Writer {
        GroupChannelsWriter {
            all_channel_groups: self.all_channel_groups()
                .map(|channel_group: &ChannelGroup| panic!("this uses relative names but expects absolute names, and all will write first byte")/*channel_group.create_channel_group_writer(header)*/)
                .collect()
        }
    }
}


pub trait WritableChannelGroup<'slf>: WritableChannels {
    fn create_channel_group_writer(&'slf self, header: &Header, channel_indices: &[usize])
        -> <Self as WritableChannels>::Writer;
}

impl<'slf> WritableChannelGroup<'slf> for AnyChannels<T>
    where AnyChannels<T>: WritableChannels<'slf>
{
    fn create_channel_group_writer(&'slf self, header: &Header, channel_indices: &[usize]) -> Self::Writer {
        self.create_writer(header)
    }
}



pub struct GroupChannelsWriter<ChannelGroupWriter> {
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
    channels: ChannelGroups<ChannelGroupReader>,

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
        let channel_groups = ChannelGroups::parse_list_to_indices(
            selected_channels_indices.iter()
                .map(|&index| &header.channels.list[index])
                .map(|selected_channel| selected_channel.name.clone())
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
    type Channels = ChannelGroups<ChannelGroupReader::Channels>;

    fn is_block_desired(&self, tile: TileCoordinates) -> bool {
        // TODO linear memory iterator
        self.channels.all_channel_groups().any(|channel_group| channel_group.is_block_desired(tile))
    }

    // for every incoming block, all the children read the lines they want into their temporary storage
    fn read_block(&mut self, header: &Header, block: &UncompressedBlock) -> UnitResult {
        for channel in self.channels.all_channel_groups_mut() { // TODO linear memory iterator
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