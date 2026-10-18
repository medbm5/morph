//! Optional components: what to download, from where, and the checksums we accept.
//! Every URL is pinned to a release; every file is SHA-256 verified before use.

/// How a downloaded asset is unpacked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unpack {
    /// Saved as-is under `files[0].1`.
    File,
    Zip,
    TarGz,
    TarXz,
}

#[derive(Debug, Clone)]
pub struct Asset {
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
    pub unpack: Unpack,
    /// (path inside the archive, destination name in the component folder).
    pub files: &'static [(&'static str, &'static str)],
}

#[derive(Debug, Clone)]
pub struct Component {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub license: &'static str,
    /// Other components that must be installed first.
    pub requires: &'static [&'static str],
    pub windows: &'static [Asset],
    pub linux: &'static [Asset],
    /// Shown when there is no download for this OS (use the system package instead).
    pub system_hint: Option<&'static str>,
}

impl Component {
    pub fn assets(&self) -> &'static [Asset] {
        if cfg!(windows) {
            self.windows
        } else {
            self.linux
        }
    }

    pub fn download_size(&self) -> u64 {
        self.assets().iter().map(|a| a.size).sum()
    }
}

const ULTRAFACE: Asset = Asset {
    url: "https://github.com/onnx/models/raw/main/validated/vision/body_analysis/ultraface/models/version-RFB-320.onnx",
    sha256: "34cd7e60aeff28744c657de7a3dc64e872d506741de66987f3426f2b79f88017",
    size: 1_270_727,
    unpack: Unpack::File,
    files: &[("", "face-ultraface-rfb320.onnx")],
};

const TEXT_DET: Asset = Asset {
    url: "https://huggingface.co/SWHL/RapidOCR/resolve/main/PP-OCRv4/en_PP-OCRv3_det_infer.onnx",
    sha256: "f139598bc2af4e4b6fe98dec11574e30edfdd91fc94ac1425c18ace3bd5a866b",
    size: 2_423_224,
    unpack: Unpack::File,
    files: &[("", "text-det-ppocrv3-en.onnx")],
};

pub static CATALOG: &[Component] = &[
    Component {
        name: "ml",
        title: "Local AI: faces and text",
        description: "ONNX Runtime (CPU) with a face detector (UltraFace) and a text detector (PP-OCRv3) for Redact. Runs entirely offline.",
        license: "ONNX Runtime MIT · UltraFace MIT · PP-OCR Apache-2.0",
        requires: &[],
        windows: &[
            Asset {
                url: "https://github.com/microsoft/onnxruntime/releases/download/v1.22.0/onnxruntime-win-x64-1.22.0.zip",
                sha256: "174c616efc0271194488642a72f1a514e01487da4dfe84c49296d66e40ebe0da",
                size: 72_368_545,
                unpack: Unpack::Zip,
                files: &[
                    ("onnxruntime-win-x64-1.22.0/lib/onnxruntime.dll", "onnxruntime.dll"),
                    ("onnxruntime-win-x64-1.22.0/lib/onnxruntime_providers_shared.dll", "onnxruntime_providers_shared.dll"),
                    ("onnxruntime-win-x64-1.22.0/LICENSE", "ONNXRUNTIME-LICENSE.txt"),
                ],
            },
            ULTRAFACE,
            TEXT_DET,
        ],
        linux: &[
            Asset {
                url: "https://github.com/microsoft/onnxruntime/releases/download/v1.22.0/onnxruntime-linux-x64-1.22.0.tgz",
                sha256: "8344d55f93d5bc5021ce342db50f62079daf39aaafb5d311a451846228be49b3",
                size: 7_798_730,
                unpack: Unpack::TarGz,
                files: &[
                    ("onnxruntime-linux-x64-1.22.0/lib/libonnxruntime.so.1.22.0", "libonnxruntime.so"),
                    ("onnxruntime-linux-x64-1.22.0/LICENSE", "ONNXRUNTIME-LICENSE.txt"),
                ],
            },
            ULTRAFACE,
            TEXT_DET,
        ],
        system_hint: None,
    },
    Component {
        name: "ocr",
        title: "Text recognition (OCR)",
        description: "Recognises English text for Copy text and searchable PDFs (PP-OCRv3).",
        license: "Apache-2.0",
        requires: &["ml"],
        windows: OCR_ASSETS,
        linux: OCR_ASSETS,
        system_hint: None,
    },
    Component {
        name: "bgremove",
        title: "Background removal",
        description: "Cuts the subject out of a photo (U²-Net small).",
        license: "Apache-2.0",
        requires: &["ml"],
        windows: BG_ASSETS,
        linux: BG_ASSETS,
        system_hint: None,
    },
];

const OCR_ASSETS: &[Asset] = &[
    Asset {
        url: "https://huggingface.co/SWHL/RapidOCR/resolve/main/PP-OCRv3/en_PP-OCRv3_rec_infer.onnx",
        sha256: "ef7abd8bd3629ae57ea2c28b425c1bd258a871b93fd2fe7c433946ade9b5d9ea",
        size: 8_967_018,
        unpack: Unpack::File,
        files: &[("", "text-rec-ppocrv3-en.onnx")],
    },
    Asset {
        url: "https://raw.githubusercontent.com/PaddlePaddle/PaddleOCR/release/2.7/ppocr/utils/en_dict.txt",
        sha256: "5662df9d2d03f0e8ca0d3b0649d6acbab904b6a14b3d3521463c71c37c668ce3",
        size: 190,
        unpack: Unpack::File,
        files: &[("", "text-rec-dict-en.txt")],
    },
];

const BG_ASSETS: &[Asset] = &[Asset {
    url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx",
    sha256: "309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8",
    size: 4_574_861,
    unpack: Unpack::File,
    files: &[("", "bg-u2netp.onnx")],
}];

pub fn find(name: &str) -> Option<&'static Component> {
    CATALOG.iter().find(|c| c.name == name)
}
