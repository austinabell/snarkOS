// Copyright (C) 2019-2021 Aleo Systems Inc.
// This file is part of the snarkOS library.

// The snarkOS library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The snarkOS library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the snarkOS library. If not, see <https://www.gnu.org/licenses/>.

// Detect the rustc channel
use rustc_version::{version_meta, Channel};

fn main() {
    #[cfg(feature = "compile_capnp_schema")]
    {
        capnpc::CompilerCommand::new()
            .file(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/network/src/message/payload.capnp"
            ))
            .output_path(".")
            .run()
            .expect("cap'n'proto network schema compilation failed");
    }

    // Set cfg flags depending on release channel
    match version_meta().unwrap().channel {
        Channel::Stable => println!("cargo:rustc-cfg=stable"),
        Channel::Beta => println!("cargo:rustc-cfg=beta"),
        Channel::Nightly => println!("cargo:rustc-cfg=nightly"),
        Channel::Dev => println!("cargo:rustc-cfg=rustc_dev"),
    }
}
