const dgram = require("dgram")
const socket = dgram.createSocket("udp4")

function repath(text) {
    return text
        .split(/\n/g)
        .map(x => x.slice(7))
        .join(" ")
        .split(" ")
        .map(x => `0x${x},`)
        .join(" ")
}


const BINDING = Buffer.from([
    0x00, 0x01, 0x00, 0x00, 0x21, 0x12, 0xa4, 0x42, 0x72, 
    0x6d, 0x49, 0x42, 0x72, 0x52, 0x64, 0x48, 0x57, 0x62, 
    0x4b, 0x2b,
])

const ALLOCATE_UDP_ERR = Buffer.from([
    0x00, 0x03, 0x00, 0x08, 0x21, 0x12, 0xa4, 0x42, 0x6d, 
    0x53, 0x44, 0x73, 0x5a, 0x4a, 0x66, 0x38, 0x75, 0x46, 
    0x34, 0x43, 0x00, 0x19, 0x00, 0x04, 0x11, 0x00, 0x00, 
    0x00,
])

const ALLOCATE_UDP = Buffer.from([
    0x00, 0x03, 0x00, 0x5c, 0x21, 0x12, 0xa4, 0x42, 0x35, 
    0x45, 0x43, 0x50, 0x79, 0x4e, 0x64, 0x2f, 0x6f, 0x75, 
    0x4e, 0x6f, 0x00, 0x19, 0x00, 0x04, 0x11, 0x00, 0x00, 
    0x00, 0x00, 0x06, 0x00, 0x09, 0x71, 0x75, 0x61, 0x73, 
    0x69, 0x70, 0x61, 0x61, 0x73, 0x00, 0x00, 0x00, 0x00, 
    0x14, 0x00, 0x12, 0x71, 0x75, 0x61, 0x73, 0x69, 0x70, 
    0x61, 0x61, 0x2e, 0x6c, 0x62, 0x78, 0x70, 0x7a, 0x2e, 
    0x63, 0x6f, 0x6d, 0x00, 0x00, 0x00, 0x15, 0x00, 0x10, 
    0x61, 0x38, 0x63, 0x32, 0x61, 0x32, 0x30, 0x65, 0x64, 
    0x36, 0x31, 0x39, 0x30, 0x32, 0x35, 0x66, 0x00, 0x08, 
    0x00, 0x14, 0x84, 0x09, 0x9f, 0x64, 0x12, 0x6a, 0x46, 
    0x32, 0x85, 0x7b, 0xf0, 0x01, 0xc6, 0xc0, 0xe3, 0xdf, 
    0xe7, 0xaa, 0x83, 0x4e,
])

socket.on("message", function (message) {
    console.log(message[0], message[1])
})

socket.send(BINDING, 3478, "localhost")