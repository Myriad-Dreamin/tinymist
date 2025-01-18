use std::{io::Read, path::Path};

use js_sys::Uint8Array;
use typst::diag::{eco_format, EcoString};
use wasm_bindgen::{prelude::*, JsValue};

use super::{PackageError, PackageRegistry, PackageSpec};

#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct ProxyContext {
    context: JsValue,
}

#[wasm_bindgen]
impl ProxyContext {
    #[wasm_bindgen(constructor)]
    pub fn new(context: JsValue) -> Self {
        Self { context }
    }

    #[wasm_bindgen(getter)]
    pub fn context(&self) -> JsValue {
        self.context.clone()
    }

    pub fn untar(&self, data: &[u8], cb: js_sys::Function) -> Result<(), JsValue> {
        let cb = move |key: String, value: &[u8], mtime: u64| -> Result<(), JsValue> {
            let key = JsValue::from_str(&key);
            let value = Uint8Array::from(value);
            let mtime = JsValue::from_f64(mtime as f64);
            cb.call3(&self.context, &key, &value, &mtime).map(|_| ())
        };

        let decompressed = flate2::read::GzDecoder::new(data);
        let mut reader = tar::Archive::new(decompressed);
        let entries = reader.entries();
        let entries = entries.map_err(|err| {
            let t = PackageError::MalformedArchive(Some(eco_format!("{err}")));
            JsValue::from_str(&format!("{t:?}"))
        })?;

        let mut buf = Vec::with_capacity(1024);
        for entry in entries {
            // Read single entry
            let mut entry = entry.map_err(|e| format!("{e:?}"))?;
            let header = entry.header();

            let is_file = header.entry_type().is_file();
            if !is_file {
                continue;
            }

            let mtime = header.mtime().unwrap_or(0);

            let path = header.path().map_err(|e| format!("{e:?}"))?;
            let path = path.to_string_lossy().as_ref().to_owned();

            let size = header.size().map_err(|e| format!("{e:?}"))?;
            buf.clear();
            buf.reserve(size as usize);
            entry.read_to_end(&mut buf).map_err(|e| format!("{e:?}"))?;

            cb(path, &buf, mtime)?
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct ProxyRegistry {
    pub context: ProxyContext,
    pub real_resolve_fn: js_sys::Function,
}

impl PackageRegistry for ProxyRegistry {
    fn resolve(&self, spec: &PackageSpec) -> Result<std::sync::Arc<Path>, PackageError> {
        // prepare js_spec
        let js_spec = js_sys::Object::new();
        js_sys::Reflect::set(&js_spec, &"name".into(), &spec.name.to_string().into()).unwrap();
        js_sys::Reflect::set(
            &js_spec,
            &"namespace".into(),
            &spec.namespace.to_string().into(),
        )
        .unwrap();
        js_sys::Reflect::set(
            &js_spec,
            &"version".into(),
            &spec.version.to_string().into(),
        )
        .unwrap();

        self.real_resolve_fn
            .call1(&self.context.clone().into(), &js_spec)
            .map_err(|e| PackageError::Other(Some(eco_format!("{:?}", e))))
            .and_then(|v| {
                if v.is_undefined() {
                    Err(PackageError::NotFound(spec.clone()))
                } else {
                    Ok(Path::new(&v.as_string().unwrap()).into())
                }
            })
    }

    // todo: provide package list for browser
    fn packages(&self) -> &[(PackageSpec, Option<EcoString>)] {
        &[]
    }
}
