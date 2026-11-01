use std::path::Path;

use stackyard_core::Result;

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    #[cfg(test)]
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;

    use stackyard_core::{pipe_path, Error, Result};
    use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    #[cfg(test)]
    use windows::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo, SE_KERNEL_OBJECT,
    };
    use windows::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        SDDL_REVISION_1,
    };
    #[cfg(test)]
    use windows::Win32::Security::DACL_SECURITY_INFORMATION;
    use windows::Win32::Security::{
        GetTokenInformation, TokenUser, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY,
        TOKEN_USER,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    pub struct Endpoint {
        server: NamedPipeServer,
        data_dir: std::path::PathBuf,
    }

    pub type Client = NamedPipeServer;

    pub fn bind(data_dir: &Path) -> Result<Endpoint> {
        let server = create_server(data_dir, true)?;
        Ok(Endpoint {
            server,
            data_dir: data_dir.to_path_buf(),
        })
    }

    impl Endpoint {
        pub async fn accept(&mut self) -> Result<Client> {
            self.server
                .connect()
                .await
                .map_err(|source| Error::io("accept engine client", source))?;
            let next = create_server(&self.data_dir, false)?;
            Ok(std::mem::replace(&mut self.server, next))
        }

        #[cfg(test)]
        pub fn dacl_sddl(&self) -> Result<String> {
            control_dacl_sddl(&self.server)
        }
    }

    fn create_server(data_dir: &Path, first: bool) -> Result<NamedPipeServer> {
        let pipe = pipe_path(data_dir);
        let descriptor = user_security_descriptor()?;
        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(0),
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        let server = unsafe {
            // SAFETY: `attributes` points at a SECURITY_ATTRIBUTES whose descriptor
            // stays alive for this call. CreateNamedPipe copies the DACL before returning.
            ServerOptions::new()
                .first_pipe_instance(first)
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(
                    pipe,
                    &mut attributes as *mut SECURITY_ATTRIBUTES as *mut c_void,
                )
        }
        .map_err(|source| map_bind_error(data_dir, source))?;
        Ok(server)
    }

    fn map_bind_error(data_dir: &Path, source: std::io::Error) -> Error {
        match source.raw_os_error() {
            Some(5 | 183 | 231) => Error::AlreadyRunning {
                data_dir: data_dir.to_path_buf(),
            },
            _ => Error::io("bind engine pipe", source),
        }
    }

    struct SecurityDescriptor(*mut c_void);

    impl Drop for SecurityDescriptor {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    let _ = LocalFree(Some(HLOCAL(self.0)));
                }
            }
        }
    }

    fn user_security_descriptor() -> Result<SecurityDescriptor> {
        let sid = current_user_sid()?;
        let sddl = format!("D:P(A;;GA;;;{sid})");
        let wide: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(wide.as_ptr()),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
            .map_err(|err| {
                Error::io(
                    "build engine pipe security descriptor",
                    std::io::Error::other(err.to_string()),
                )
            })?;
        }
        Ok(SecurityDescriptor(descriptor.0))
    }

    fn current_user_sid() -> Result<String> {
        unsafe {
            let mut token = windows::Win32::Foundation::HANDLE::default();
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).map_err(|err| {
                Error::io("open process token", std::io::Error::other(err.to_string()))
            })?;
            let mut needed = 0u32;
            let _ = GetTokenInformation(token, TokenUser, None, 0, &mut needed);
            let mut buffer = vec![0u8; needed as usize];
            GetTokenInformation(
                token,
                TokenUser,
                Some(buffer.as_mut_ptr().cast()),
                needed,
                &mut needed,
            )
            .map_err(|err| {
                Error::io(
                    "read process token user",
                    std::io::Error::other(err.to_string()),
                )
            })?;
            let user = &*(buffer.as_ptr() as *const TOKEN_USER);
            let mut sid = windows::core::PWSTR::null();
            ConvertSidToStringSidW(user.User.Sid, &mut sid).map_err(|err| {
                Error::io("convert user sid", std::io::Error::other(err.to_string()))
            })?;
            let text = sid.to_string().map_err(|err| {
                Error::io("read user sid", std::io::Error::other(err.to_string()))
            })?;
            let _ = LocalFree(Some(HLOCAL(sid.as_ptr().cast())));
            Ok(text)
        }
    }

    #[cfg(test)]
    pub fn control_dacl_sddl(server: &NamedPipeServer) -> Result<String> {
        unsafe {
            let mut descriptor = PSECURITY_DESCRIPTOR::default();
            let status = GetSecurityInfo(
                windows::Win32::Foundation::HANDLE(server.as_raw_handle()),
                SE_KERNEL_OBJECT,
                DACL_SECURITY_INFORMATION,
                None,
                None,
                None,
                None,
                Some(&mut descriptor),
            );
            if status.is_err() {
                return Err(Error::io(
                    "read engine pipe security descriptor",
                    std::io::Error::from_raw_os_error(status.0 as i32),
                ));
            }
            let mut wide = windows::core::PWSTR::null();
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut wide,
                None,
            )
            .map_err(|err| {
                Error::io(
                    "stringify engine pipe security descriptor",
                    std::io::Error::other(err.to_string()),
                )
            })?;
            let text = wide.to_string().map_err(|err| {
                Error::io(
                    "read engine pipe security descriptor",
                    std::io::Error::other(err.to_string()),
                )
            })?;
            let _ = LocalFree(Some(HLOCAL(wide.as_ptr().cast())));
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
            Ok(text)
        }
    }
}

#[cfg(unix)]
mod platform {
    use std::path::Path;

    use stackyard_core::{socket_path, Error, Result};
    use tokio::net::{UnixListener, UnixStream};

    pub struct Endpoint {
        listener: UnixListener,
    }

    pub type Client = UnixStream;

    pub fn bind(data_dir: &Path) -> Result<Endpoint> {
        let socket = socket_path(data_dir);
        if socket.exists() {
            match std::os::unix::net::UnixStream::connect(&socket) {
                Ok(_) => {
                    return Err(Error::AlreadyRunning {
                        data_dir: data_dir.to_path_buf(),
                    });
                }
                Err(_) => {
                    let _ = std::fs::remove_file(&socket);
                }
            }
        }
        let listener = UnixListener::bind(&socket)
            .map_err(|source| Error::io("bind engine socket", source))?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
            .map_err(|source| Error::io("restrict engine socket permissions", source))?;
        Ok(Endpoint { listener })
    }

    impl Endpoint {
        pub async fn accept(&mut self) -> Result<Client> {
            let (stream, _) = self
                .listener
                .accept()
                .await
                .map_err(|source| Error::io("accept engine client", source))?;
            Ok(stream)
        }
    }
}

pub use platform::Endpoint;

impl Endpoint {
    pub fn bind(data_dir: &Path) -> Result<Self> {
        platform::bind(data_dir)
    }
}

#[cfg(all(test, windows))]
mod tests {
    #[tokio::test]
    async fn pipe_dacl_does_not_grant_other_local_users() {
        let path = std::env::temp_dir().join(format!(
            "stackyard-dacl-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let dir = stackyard_core::prepare_data_dir(&path).expect("temp dir");
        let endpoint = super::Endpoint::bind(&dir).expect("bind");
        let sddl = endpoint.dacl_sddl().expect("dacl");
        assert!(!sddl.contains("S-1-1-0"), "everyone ace: {sddl}");
        assert!(
            !sddl.contains("S-1-5-11"),
            "authenticated users ace: {sddl}"
        );
        assert!(!sddl.contains("S-1-5-32-544"), "administrators ace: {sddl}");
        assert!(
            sddl.contains("S-1-5-21-"),
            "current user ace missing: {sddl}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
