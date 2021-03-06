use super::{Event, Rx, Tx};
use bytes::BytesMut;
use futures::prelude::*;
use std::net::SocketAddr;
use std::task::{Context, Poll};
use std::collections::{HashMap, HashSet};
use std::{io::Error, pin::Pin, sync::Arc, io::ErrorKind};
use tokio::{io::AsyncRead, io::AsyncWrite, net::TcpStream};
use transport::{Flag, Payload, Transport};

// type
type Frame = HashMap<String, Arc<Payload>>;

/// 数据搬运
///
/// 用于处理和交换中心之间的通讯，
/// 获取流数据和反馈事件.
///
/// TODO: 单路TCP负载能力有限，
/// 计划使用多路合并来提高传输能力;
pub struct Porter {
    peer: HashMap<String, Vec<Tx>>,
    channel: HashSet<String>,
    transport: Transport,
    video_frame: Frame,
    audio_frame: Frame,
    stream: TcpStream,
    receiver: Rx,
    frame: Frame
}

impl Porter {
    /// 创建数据搬运实例
    ///
    /// 通过指定远程交换中心地址和传入一个读取管道来完成创建，
    /// 外部通过管道向这个模块传递一些基础事件.
    pub async fn new(addr: SocketAddr, receiver: Rx) -> Result<Self, Error> {
        Ok(Self {
            receiver,
            peer: HashMap::new(),
            frame: HashMap::new(),
            channel: HashSet::new(),
            transport: Transport::new(),
            video_frame: HashMap::new(),
            audio_frame: HashMap::new(),
            stream: TcpStream::connect(addr).await?,
        })
    }

    /// 从TcpSocket读取数据
    ///
    /// 单次最大从缓冲区获取2048字节，
    /// 并转换为BytesMut返回.
    ///
    /// TODO: 目前存在重复申请缓冲区的情况，有优化空间；
    #[rustfmt::skip]
    fn read<'b>(&mut self, ctx: &mut Context<'b>) -> Option<BytesMut> {
        let mut receiver = [0u8; 2048];
        match Pin::new(&mut self.stream).poll_read(ctx, &mut receiver) {
            Poll::Ready(Ok(s)) if s > 0 => Some(BytesMut::from(&receiver[0..s])),
            _ => None,
        }
    }

    /// 发送数据到TcpSocket
    ///
    /// 如果出现未完全写入的情况，
    /// 这里将重复重试，直到写入完成.
    #[rustfmt::skip]
    fn send<'b>(&mut self, ctx: &mut Context<'b>, data: &[u8]) -> Result<(), Error> {
        let mut offset: usize = 0;
        let length = data.len();
        loop {
            match Pin::new(&mut self.stream).poll_write(ctx, &data) {
                Poll::Ready(Err(e)) => { return Err(Error::new(ErrorKind::NotConnected, e)); }, 
                Poll::Ready(Ok(s)) => match offset + s >= length {
                    false => { offset += s; },
                    true => { break; }
                }, _ => (),
            }
        }

        Ok(())
    }

    /// 刷新缓冲区并将Tcp数据推送到远端
    ///
    /// 重复尝试刷新，
    /// 直到数据完全发送到对端.
    #[rustfmt::skip]
    fn flush<'b>(&mut self, ctx: &mut Context<'b>) -> Result<(), Error> {
        loop {
            match Pin::new(&mut self.stream).poll_flush(ctx) {
                Poll::Ready(Err(e)) => { return Err(Error::new(ErrorKind::NotConnected, e)); },
                Poll::Ready(Ok(_)) => { break; },
                _ => (),
            }
        }

        Ok(())
    }

    /// 处理远程订阅
    ///
    /// 将订阅事件发送到交换中心，
    /// 通知这个实例已经订阅了这个频道.
    /// 这里需要注意的是，如果已经订阅的频道，
    /// 这个地方将跳过，不需要重复订阅.
    #[rustfmt::skip]
    fn peer_subscribe<'b>(&mut self, ctx: &mut Context<'b>, name: String) -> Result<(), Error> {
        if self.channel.contains(&name) { return Ok(()); }
        self.channel.insert(name.clone());
        self.send(ctx, &Transport::encoder(Transport::packet(Payload {
            name,
            timestamp: 0,
            data: BytesMut::new(),
        }), Flag::FlvPull))?;
        self.flush(ctx)?;
        Ok(())
    }

    /// 订阅频道
    ///
    /// 将外部可写管道添加到频道列表中，
    /// 将管道和频道对应绑定.
    fn subscribe<'b>(&mut self, ctx: &mut Context<'b>, name: String, sender: Tx) -> Result<(), Error> {
        self.peer_subscribe(ctx, name.clone())?;

        // 发送媒体信息
        // FLV的特殊处理，
        // FLV需要这个信息完成播放.
        if let Some(payload) = self.frame.get(&name) {
            let event = Event::Bytes(Flag::FlvFrame, payload.clone());
            sender.send(event).map_err(drop).unwrap();
        }

        // 发送首帧视频
        // FLV的特殊处理，
        // FLV头帧视频需要H264的配置信息.
        if let Some(payload) = self.video_frame.get(&name) {
            let event = Event::Bytes(Flag::FlvVideo, payload.clone());
            sender.send(event).map_err(drop).unwrap();
        }

        // 发送首帧音频
        // FLV的特殊处理，
        // FLV头帧音频需要H264的配置信息.
        if let Some(payload) = self.audio_frame.get(&name) {
            let event = Event::Bytes(Flag::FlvAudio, payload.clone());
            sender.send(event).map_err(drop).unwrap();
        }

        // 将客户端和频道绑定，
        // 方便后续频道的操作直接对应到
        // 客户端，失效时删除即可.
        self.peer.entry(name)
            .or_insert_with(Vec::new)
            .push(sender);
        Ok(())
    }

    /// 处理数据负载
    ///
    /// 将数据负载发送给每个订阅了此频道的管道,
    /// 如果发送失败，这个地方目前当失效处理，
    /// 直接从订阅列表中删除这个管道.
    #[rustfmt::skip]
    fn process_payload(frame: &mut Frame, peer: &mut Vec<Tx>, flag: Flag, payload: Arc<Payload>) {
        let mut failure = Vec::new();
        if let Flag::FlvFrame = flag {
            frame.entry(payload.name.clone()).or_insert_with(|| {
                payload.clone()
            });
        }

        // 遍历所有的客户端，
        // 将消息路由到相应的客户端.
        for (index, tx) in peer.iter().enumerate() {
            if tx.send(Event::Bytes(flag, payload.clone())).is_err() {
                failure.push(index);
            }
        }

        // 删除失效的管道
        // 因为这里没法确定管道是因为
        // 什么原因也失效，也没必要知道，
        // 直接删除掉无法工作的管道即可.
        for index in failure {
            peer.remove(index);
        }
    }

    /// 处理读取管道
    ///
    /// 处理外部传入的相关事件，
    /// 处理到内部，比如订阅频道.
    fn process_receiver<'b>(&mut self, ctx: &mut Context<'b>) -> Result<(), Error> {
        while let Poll::Ready(Some(event)) = Pin::new(&mut self.receiver).poll_next(ctx) {
            if let Event::Subscribe(name, sender) = event {
                self.subscribe(ctx, name, sender)?;
            }
        }

        Ok(())
    }

    /// 处理TcpSocket数据
    ///
    /// 这里将数据从TcpSocket中读取处理，
    /// 并解码数据，直到拆分成单个负载，
    /// 然后再进行相应的处理.
    #[rustfmt::skip]
    fn process_socket<'b>(&mut self, ctx: &mut Context<'b>) {
        while let Some(chunk) = self.read(ctx) {
            if let Some(result) = self.transport.decoder(chunk) {
                for (flag, message) in result {
                    if let Ok(payload) = Transport::parse(message) {
                        if let Some(peer) = self.peer.get_mut(&payload.name) {
                            Self::process_payload(&mut self.frame, peer, flag, Arc::new(payload));
                        }
                    }
                }
            }
        }
    }

    /// 顺序处理多个任务
    ///
    /// 处理外部的事件通知，
    /// 处理内部TcpSocket数据.
    #[rustfmt::skip]
    fn process<'b>(&mut self, ctx: &mut Context<'b>) -> Result<(), Error> {
        self.process_receiver(ctx)?;
        self.process_socket(ctx);
        Ok(())
    }
}

impl Future for Porter {
    type Output = Result<(), Error>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output> {
        match self.get_mut().process(ctx) {
            Ok(_) => Poll::Pending,
            Err(_) => Poll::Ready(Ok(()))
        }
    }
}
