use anyhow::Result;
use std::marker::PhantomData;
use std::mem;

use crate::runtime::AsyncKernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct VectorSink<T: Clone + std::fmt::Debug + Sync + 'static> {
    items: Vec<T>,
}

impl<T: Clone + std::fmt::Debug + Send + Sync + 'static> VectorSink<T> {
    pub fn new(capacity: usize) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("VectorSink").build(),
            StreamIoBuilder::new()
                .add_stream_input("in", mem::size_of::<T>())
                .build(),
            MessageIoBuilder::<Self>::new().build(),
            VectorSink {
                items: Vec::<T>::with_capacity(capacity),
            },
        )
    }

    pub fn items(&self) -> &Vec<T> {
        &self.items
    }
}

#[async_trait]
impl<T: Clone + std::fmt::Debug + Send + Sync + 'static> AsyncKernel for VectorSink<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        loop {
            let i = sio.input(0).slice::<T>();
            if i.is_empty() {
                break;
            }

            self.items.extend_from_slice(i);

            sio.input(0).consume(i.len());
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}

pub struct VectorSinkBuilder<T: Clone + std::fmt::Debug + Sync + 'static> {
    capacity: usize,
    _foo: PhantomData<T>,
}

impl<T: Clone + std::fmt::Debug + Send + Sync + 'static> VectorSinkBuilder<T> {
    pub fn new() -> VectorSinkBuilder<T> {
        VectorSinkBuilder {
            capacity: 8192,
            _foo: PhantomData,
        }
    }

    pub fn init_capacity(mut self, n: usize) -> VectorSinkBuilder<T> {
        self.capacity = n;
        self
    }

    pub fn build(self) -> Block {
        VectorSink::<T>::new(self.capacity)
    }
}

impl<T: Clone + std::fmt::Debug + Send + Sync + 'static> Default for VectorSinkBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}