//! Performance benchmarks for rsws WebSocket library.
//!
//! Run with: `cargo bench`

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use rsws::protocol::assembler::MessageAssembler;
use rsws::protocol::frame::Frame;
use rsws::protocol::handshake::{compute_accept_key, HandshakeRequest, HandshakeResponse};
use rsws::protocol::mask::{apply_mask, apply_mask_fast, apply_mask_simd};
use rsws::protocol::OpCode;
use rsws::Config;

// =============================================================================
// Frame Parsing Benchmarks
// =============================================================================

fn create_unmasked_frame(payload_size: usize) -> Vec<u8> {
    let payload = vec![0xAB; payload_size];
    let frame = Frame::binary(payload);
    let mut buf = vec![0u8; frame.wire_size(false)];
    frame.write(&mut buf, None).unwrap();
    buf
}

fn create_masked_frame(payload_size: usize) -> Vec<u8> {
    let payload = vec![0xAB; payload_size];
    let frame = Frame::binary(payload);
    let mask = [0x37, 0xfa, 0x21, 0x3d];
    let mut buf = vec![0u8; frame.wire_size(true)];
    frame.write(&mut buf, Some(mask)).unwrap();
    buf
}

fn bench_frame_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_parsing");

    // Small frames (10 bytes payload) - Target: <50ns
    let small_unmasked = create_unmasked_frame(10);
    let small_masked = create_masked_frame(10);

    group.throughput(Throughput::Bytes(10));
    group.bench_function("small_10b_unmasked", |b| {
        b.iter(|| Frame::parse(black_box(&small_unmasked)))
    });

    group.bench_function("small_10b_masked", |b| {
        b.iter(|| Frame::parse(black_box(&small_masked)))
    });

    // Medium frames (1KB payload)
    let medium_unmasked = create_unmasked_frame(1024);
    let medium_masked = create_masked_frame(1024);

    group.throughput(Throughput::Bytes(1024));
    group.bench_function("medium_1kb_unmasked", |b| {
        b.iter(|| Frame::parse(black_box(&medium_unmasked)))
    });

    group.bench_function("medium_1kb_masked", |b| {
        b.iter(|| Frame::parse(black_box(&medium_masked)))
    });

    // Large frames (64KB payload)
    let large_unmasked = create_unmasked_frame(65536);
    let large_masked = create_masked_frame(65536);

    group.throughput(Throughput::Bytes(65536));
    group.bench_function("large_64kb_unmasked", |b| {
        b.iter(|| Frame::parse(black_box(&large_unmasked)))
    });

    group.bench_function("large_64kb_masked", |b| {
        b.iter(|| Frame::parse(black_box(&large_masked)))
    });

    group.finish();
}

// =============================================================================
// Masking Benchmarks
// =============================================================================

fn bench_masking(c: &mut Criterion) {
    let mut group = c.benchmark_group("masking");
    let mask = [0x37, 0xfa, 0x21, 0x3d];

    // Small payload (64 bytes)
    let small_size = 64;
    group.throughput(Throughput::Bytes(small_size as u64));

    group.bench_function("apply_mask_64b", |b| {
        let mut data = vec![0xAB; small_size];
        b.iter(|| {
            apply_mask(black_box(&mut data), mask);
        })
    });

    group.bench_function("apply_mask_fast_64b", |b| {
        let mut data = vec![0xAB; small_size];
        b.iter(|| {
            apply_mask_fast(black_box(&mut data), mask);
        })
    });

    group.bench_function("apply_mask_simd_64b", |b| {
        let mut data = vec![0xAB; small_size];
        b.iter(|| {
            apply_mask_simd(black_box(&mut data), mask);
        })
    });

    // Medium payload (1KB)
    let medium_size = 1024;
    group.throughput(Throughput::Bytes(medium_size as u64));

    group.bench_function("apply_mask_1kb", |b| {
        let mut data = vec![0xAB; medium_size];
        b.iter(|| {
            apply_mask(black_box(&mut data), mask);
        })
    });

    group.bench_function("apply_mask_fast_1kb", |b| {
        let mut data = vec![0xAB; medium_size];
        b.iter(|| {
            apply_mask_fast(black_box(&mut data), mask);
        })
    });

    group.bench_function("apply_mask_simd_1kb", |b| {
        let mut data = vec![0xAB; medium_size];
        b.iter(|| {
            apply_mask_simd(black_box(&mut data), mask);
        })
    });

    // Large payload (64KB) - Target: >2GB/s throughput
    let large_size = 65536;
    group.throughput(Throughput::Bytes(large_size as u64));

    group.bench_function("apply_mask_64kb", |b| {
        let mut data = vec![0xAB; large_size];
        b.iter(|| {
            apply_mask(black_box(&mut data), mask);
        })
    });

    group.bench_function("apply_mask_fast_64kb", |b| {
        let mut data = vec![0xAB; large_size];
        b.iter(|| {
            apply_mask_fast(black_box(&mut data), mask);
        })
    });

    group.bench_function("apply_mask_simd_64kb", |b| {
        let mut data = vec![0xAB; large_size];
        b.iter(|| {
            apply_mask_simd(black_box(&mut data), mask);
        })
    });

    // Very large payload (1MB) for throughput measurement
    let huge_size = 1024 * 1024;
    group.throughput(Throughput::Bytes(huge_size as u64));

    group.bench_function("apply_mask_1mb", |b| {
        let mut data = vec![0xAB; huge_size];
        b.iter(|| {
            apply_mask(black_box(&mut data), mask);
        })
    });

    group.bench_function("apply_mask_fast_1mb", |b| {
        let mut data = vec![0xAB; huge_size];
        b.iter(|| {
            apply_mask_fast(black_box(&mut data), mask);
        })
    });

    group.bench_function("apply_mask_simd_1mb", |b| {
        let mut data = vec![0xAB; huge_size];
        b.iter(|| {
            apply_mask_simd(black_box(&mut data), mask);
        })
    });

    group.finish();
}

// =============================================================================
// Handshake Benchmarks
// =============================================================================

fn bench_handshake(c: &mut Criterion) {
    let mut group = c.benchmark_group("handshake");

    // compute_accept_key performance
    let key = "dGhlIHNhbXBsZSBub25jZQ==";
    group.bench_function("compute_accept_key", |b| {
        b.iter(|| compute_accept_key(black_box(key)))
    });

    // HandshakeRequest parsing
    let request = b"GET /chat HTTP/1.1\r\n\
        Host: server.example.com\r\n\
        Upgrade: websocket\r\n\
        Connection: Upgrade\r\n\
        Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
        Sec-WebSocket-Version: 13\r\n\
        Origin: http://example.com\r\n\
        Sec-WebSocket-Protocol: chat, superchat\r\n\
        \r\n";

    group.bench_function("parse_request", |b| {
        b.iter(|| HandshakeRequest::parse(black_box(request)))
    });

    // HandshakeRequest with validation
    group.bench_function("parse_and_validate_request", |b| {
        b.iter(|| {
            let req = HandshakeRequest::parse(black_box(request)).unwrap();
            req.validate()
        })
    });

    // HandshakeResponse generation
    let req = HandshakeRequest::parse(request).unwrap();
    group.bench_function("generate_response", |b| {
        b.iter(|| HandshakeResponse::from_request(black_box(&req)))
    });

    // HandshakeResponse write
    let resp = HandshakeResponse::from_request(&req);
    group.bench_function("write_response", |b| {
        b.iter(|| {
            let mut buf = Vec::with_capacity(256);
            resp.write(&mut buf);
            black_box(buf)
        })
    });

    // Full handshake: parse request -> generate response -> write
    group.bench_function("full_handshake", |b| {
        b.iter(|| {
            let req = HandshakeRequest::parse(black_box(request)).unwrap();
            req.validate().unwrap();
            let resp = HandshakeResponse::from_request(&req);
            let mut buf = Vec::with_capacity(256);
            resp.write(&mut buf);
            black_box(buf)
        })
    });

    // Parse server response
    let response = b"HTTP/1.1 101 Switching Protocols\r\n\
        Upgrade: websocket\r\n\
        Connection: Upgrade\r\n\
        Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n\
        Sec-WebSocket-Protocol: chat\r\n\
        \r\n";

    group.bench_function("parse_response", |b| {
        b.iter(|| HandshakeResponse::parse(black_box(response)))
    });

    group.finish();
}

// =============================================================================
// Message Reassembly Benchmarks
// =============================================================================

fn bench_reassembly(c: &mut Criterion) {
    let mut group = c.benchmark_group("reassembly");

    // Single large frame
    group.throughput(Throughput::Bytes(65536));
    group.bench_function("single_frame_64kb", |b| {
        b.iter(|| {
            let config = Config::default();
            let mut assembler = MessageAssembler::new(config);
            let frame = Frame::binary(vec![0xAB; 65536]);
            assembler.push(frame).unwrap()
        })
    });

    // Multiple fragments
    group.bench_function("10_fragments_64kb", |b| {
        b.iter(|| {
            let config = Config::default();
            let mut assembler = MessageAssembler::new(config);
            for i in 0..9 {
                let frame = Frame::new(
                    false,
                    if i == 0 {
                        OpCode::Binary
                    } else {
                        OpCode::Continuation
                    },
                    vec![0xAB; 6554],
                );
                assembler.push(frame).unwrap();
            }
            let frame = Frame::new(true, OpCode::Continuation, vec![0xAB; 6554]);
            assembler.push(frame).unwrap()
        })
    });

    group.finish();
}

// =============================================================================
// Criterion Setup
// =============================================================================

criterion_group!(
    benches,
    bench_frame_parsing,
    bench_masking,
    bench_handshake,
    bench_reassembly
);

criterion_main!(benches);
