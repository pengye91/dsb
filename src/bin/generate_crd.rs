// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
#[cfg(feature = "kubernetes")]
use dsb::k8s::crd::Sandbox;
#[cfg(feature = "kubernetes")]
use kube::CustomResourceExt;

fn main() {
    #[cfg(feature = "kubernetes")]
    {
        let crd = Sandbox::crd();
        println!("{}", serde_yaml::to_string(&crd).unwrap());
    }
    #[cfg(not(feature = "kubernetes"))]
    {
        println!("This tool requires the 'kubernetes' feature to be enabled");
    }
}
