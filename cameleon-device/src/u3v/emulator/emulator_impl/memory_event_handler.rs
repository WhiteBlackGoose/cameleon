use async_std::channel::{self, Receiver, Sender};
use futures::channel::oneshot;

use cameleon_impl::memory::{prelude::*, MemoryObserver};

use super::{
    control_module::Worker,
    control_protocol::*,
    memory::{Memory, ABRM, SIRM, SIRM_ALIGNMENT},
    signal::*,
};

const MEMORY_EVENT_CHANNEL_CAPACITY: usize = 100;

#[derive(Clone)]
pub(super) struct MemoryEventHandler {
    rx: Receiver<MemoryEvent>,
}

impl MemoryEventHandler {
    /// Construct `MemoryEventHandler` while registering observers to memory.
    pub(super) async fn new(memory: &mut Memory) -> Self {
        let (tx, rx) = channel::bounded(MEMORY_EVENT_CHANNEL_CAPACITY);
        MemoryEvent::register_events(memory, &tx);

        MemoryEventHandler { rx }
    }

    /// Handle write events, return Some(error_ack) if an error occurs while handling write events.
    pub(super) async fn handle_events(
        &self,
        worker: &Worker,
        scd_kind: cmd::ScdKind,
    ) -> Result<(), ack::ErrorAck> {
        let mut error_ack = Ok(());

        while let Ok(event) = self.rx.try_recv() {
            let ack = event.process(worker, scd_kind).await;
            error_ack = error_ack.and(ack);
        }
        error_ack
    }
}

macro_rules! define_handler {
    ($handler_name:ident, $reg:path, $event:path) => {
        struct $handler_name {
            sender: Sender<MemoryEvent>,
        }

        impl $handler_name {
            fn register(memory: &mut Memory, tx: &Sender<MemoryEvent>) {
                memory.register_observer::<$reg, _>($handler_name { sender: tx.clone() });
            }

            #[allow(dead_code)]
            fn read(
                memory: &Memory,
                scd_kind: cmd::ScdKind,
            ) -> Result<<$reg as Register>::Ty, ack::ErrorAck> {
                read_memory::<$reg>(memory, scd_kind)
            }

            #[allow(dead_code)]
            fn write(
                val: <$reg as Register>::Ty,
                memory: &mut Memory,
                scd_kind: cmd::ScdKind,
            ) -> Result<(), ack::ErrorAck> {
                write_memory::<$reg>(val, memory, scd_kind)
            }
        }

        impl MemoryObserver for $handler_name {
            fn update(&self) {
                if let Err(e) = self.sender.try_send($event) {
                    log::warn!("memory observer error: {}", e);
                }
            }
        }
    };
}

define_handler!(
    TimestampLatchHandler,
    ABRM::TimestampLatch,
    MemoryEvent::TimestampLatch
);
impl TimestampLatchHandler {
    /// Handle events caused by writes to `TiemStampLatch` regsiter.
    ///
    /// If 1 is written to `TiemStampLatch`, `TimeStamp` register must be updated with current device time stamp.
    async fn handle_events(worker: &Worker, scd_kind: cmd::ScdKind) -> Result<(), ack::ErrorAck> {
        let mut memory = worker.memory.lock().await;
        let value = Self::read(&memory, scd_kind)?;
        // Write any number other than 1 cause error.
        if value != 1 {
            return Err(ack::ErrorAck::new(ack::GenCpStatus::GenericError, scd_kind).into());
        }

        // Write current time stamp to `TimeStamp` register.
        let timestamp_ns = worker.timestamp.as_nanos().await;
        write_memory::<ABRM::Timestamp>(timestamp_ns, &mut memory, scd_kind)?;

        drop(memory);

        // Send signal to [`super::event_module::EventModule`] to notify `TimeStamp` register is updated.
        let signal = EventSignal::UpdateTimestamp(timestamp_ns);
        worker.try_send_signal(signal);

        Ok(())
    }
}

define_handler!(SiControlHandler, SIRM::Control, MemoryEvent::SiControl);
impl SiControlHandler {
    async fn handle_events(worker: &Worker, scd_kind: cmd::ScdKind) -> Result<(), ack::ErrorAck> {
        let value = Self::read(&*worker.memory.lock().await, scd_kind)?;

        if value == 1 {
            Self::enable_sirm(worker, scd_kind).await
        } else if value == 0 {
            Self::disable_sirm(worker, scd_kind).await;
            Ok(())
        } else {
            Err(ack::ErrorAck::new(ack::GenCpStatus::GenericError, scd_kind).into())
        }
    }

    /// Handle events caused by `SIRM::Control` is set to 1.
    async fn enable_sirm(worker: &Worker, scd_kind: cmd::ScdKind) -> Result<(), ack::ErrorAck> {
        // 1. Verify SIRM integrity.

        // 1.1 Verify alignement restriction.
        let mut res = Self::verify_alignment(worker, scd_kind).await;

        // 1.2 Verify specified size of trailer/leader/payload is greater than
        res = res.and(Self::verify_size(worker, scd_kind).await);

        // If verification failed, set 0 to SiControl and return.
        if res.is_err() {
            Self::write(0, &mut *worker.memory.lock().await, scd_kind)?;
            return res;
        }

        // Send signal to [`super::stream_module::StreamModule`] to enable it.
        let signal = StreamSignal::Enable;
        worker.try_send_signal(signal);

        Ok(())
    }

    /// Handle events caused by `SIRM::Control` is set to 0.
    async fn disable_sirm(worker: &Worker, _: cmd::ScdKind) {
            let (completed_tx, completed_rx) = oneshot::channel();
        let signal = StreamSignal::Disable(completed_tx);
        worker.try_send_signal(signal);
        completed_rx.await.ok();
    }

    /// Verify specified sizes of writable registers have correct alignment.
    async fn verify_alignment(
        worker: &Worker,
        scd_kind: cmd::ScdKind,
    ) -> Result<(), ack::ErrorAck> {
        use SIRM::*;

        let memory = worker.memory.lock().await;
        let alignement = SIRM_ALIGNMENT as u32;
        if read_memory::<MaximumLeaderSize>(&memory, scd_kind)? % alignement != 0
            || read_memory::<PayloadTransferSize>(&memory, scd_kind)? % alignement != 0
            || read_memory::<PayloadFinalTransferSize1>(&memory, scd_kind)? % alignement != 0
            || read_memory::<PayloadFinalTransferSize2>(&memory, scd_kind)? % alignement != 0
            || read_memory::<MaximumTrailerSize>(&memory, scd_kind)? % alignement != 0
        {
            Err(ack::ErrorAck::new(
                ack::UsbSpecificStatus::InvalidSiState,
                scd_kind,
            ))
        } else {
            Ok(())
        }
    }

    /// Verify specified sizes of writable registers are greater than required sizes.
    async fn verify_size(worker: &Worker, scd_kind: cmd::ScdKind) -> Result<(), ack::ErrorAck> {
        use SIRM::*;

        let memory = worker.memory.lock().await;
        // Verify leader size.
        if read_memory::<MaximumLeaderSize>(&memory, scd_kind)?
            < read_memory::<RequiredLeaderSize>(&memory, scd_kind)?
        {
            return Err(ack::ErrorAck::new(
                ack::UsbSpecificStatus::InvalidSiState,
                scd_kind,
            ));
        }

        // Verify trailer size.
        if read_memory::<MaximumTrailerSize>(&memory, scd_kind)?
            < read_memory::<RequiredTrailerSize>(&memory, scd_kind)?
        {
            return Err(ack::ErrorAck::new(
                ack::UsbSpecificStatus::InvalidSiState,
                scd_kind,
            ));
        }

        // Verify payload size.
        let specified_payload_size = read_memory::<PayloadTransferSize>(&memory, scd_kind)? as u64
            * read_memory::<PayloadTransferCount>(&memory, scd_kind)? as u64
            + read_memory::<PayloadFinalTransferSize1>(&memory, scd_kind)? as u64
            + read_memory::<PayloadFinalTransferSize2>(&memory, scd_kind)? as u64;

        if specified_payload_size < read_memory::<RequiredPayloadSize>(&memory, scd_kind)? {
            return Err(ack::ErrorAck::new(
                ack::UsbSpecificStatus::InvalidSiState,
                scd_kind,
            ));
        }

        Ok(())
    }
}

enum MemoryEvent {
    TimestampLatch,
    SiControl,
}

impl MemoryEvent {
    async fn process(self, worker: &Worker, scd_kind: cmd::ScdKind) -> Result<(), ack::ErrorAck> {
        use MemoryEvent::*;
        match self {
            TimestampLatch => TimestampLatchHandler::handle_events(worker, scd_kind).await,
            SiControl => SiControlHandler::handle_events(worker, scd_kind).await,
        }
    }

    fn register_events(memory: &mut Memory, sender: &Sender<Self>) {
        TimestampLatchHandler::register(memory, sender);
        SiControlHandler::register(memory, sender);
    }
}

fn read_memory<T: Register>(
    memory: &Memory,
    scd_kind: cmd::ScdKind,
) -> Result<T::Ty, ack::ErrorAck> {
    memory.read::<T>().map_err(|e| {
        log::error!("failed to read memory: {}", e);
        ack::ErrorAck::new(ack::GenCpStatus::GenericError, scd_kind)
    })
}

fn write_memory<T: Register>(
    val: T::Ty,
    memory: &mut Memory,
    scd_kind: cmd::ScdKind,
) -> Result<(), ack::ErrorAck> {
    memory.write::<T>(val).map_err(|e| {
        log::error!("failed to write memory: {}", e);
        ack::ErrorAck::new(ack::GenCpStatus::GenericError, scd_kind)
    })
}
