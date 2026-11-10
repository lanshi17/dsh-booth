use super::*;
use pretty_assertions::assert_eq;
use windows_sys::Win32::Security::EqualSid;
use windows_sys::Win32::Security::TokenRestrictedSids;

unsafe fn token_has_restricting_sid(token: HANDLE, expected_sid: *mut c_void) -> Result<bool> {
    let mut needed = 0;
    GetTokenInformation(
        token,
        TokenRestrictedSids,
        std::ptr::null_mut(),
        0,
        &mut needed,
    );
    if needed == 0 {
        return Err(anyhow!(
            "GetTokenInformation(TokenRestrictedSids) size query failed: {}",
            GetLastError()
        ));
    }

    let mut buffer = vec![0_u8; needed as usize];
    if GetTokenInformation(
        token,
        TokenRestrictedSids,
        buffer.as_mut_ptr().cast(),
        needed,
        &mut needed,
    ) == 0
    {
        return Err(anyhow!(
            "GetTokenInformation(TokenRestrictedSids) failed: {}",
            GetLastError()
        ));
    }

    let group_count = std::ptr::read_unaligned(buffer.as_ptr().cast::<u32>()) as usize;
    let after_count = buffer.as_ptr().add(std::mem::size_of::<u32>()) as usize;
    let align = std::mem::align_of::<SID_AND_ATTRIBUTES>();
    let entries_addr = (after_count + (align - 1)) & !(align - 1);
    let restricting_sids =
        std::slice::from_raw_parts(entries_addr as *const SID_AND_ATTRIBUTES, group_count);
    Ok(restricting_sids
        .iter()
        .any(|entry| EqualSid(entry.Sid, expected_sid) != 0))
}

#[test]
fn legacy_tokens_do_not_accept_everyone_as_a_write_restriction() -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::io::FromRawHandle;
    use std::os::windows::io::OwnedHandle;

    let capability = LocalSid::from_string("S-1-5-21-10-20-30-40")?;
    unsafe {
        let base = OwnedHandle::from_raw_handle(get_current_token_for_restriction()?);
        let mut everyone = world_sid()?;
        let mut logon = get_logon_sid_bytes(base.as_raw_handle())?;
        for create_token in [
            create_workspace_write_token_with_caps_from,
            create_readonly_token_with_caps_from,
        ] {
            let token = OwnedHandle::from_raw_handle(create_token(
                base.as_raw_handle(),
                &[capability.as_ptr()],
            )?);
            assert!(token_has_restricting_sid(
                token.as_raw_handle(),
                capability.as_ptr()
            )?);
            assert!(token_has_restricting_sid(
                token.as_raw_handle(),
                logon.as_mut_ptr().cast()
            )?);
            assert!(!token_has_restricting_sid(
                token.as_raw_handle(),
                everyone.as_mut_ptr().cast()
            )?);
        }
    }
    Ok(())
}

#[test]
fn legacy_tokens_deny_public_acl_mutations_without_a_root_capability() -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::io::FromRawHandle;
    use std::os::windows::io::OwnedHandle;
    use windows_sys::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
    use windows_sys::Win32::Security::Authorization::SDDL_REVISION_1;
    use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;
    use windows_sys::Win32::Security::DuplicateToken;
    use windows_sys::Win32::Security::ImpersonateLoggedOnUser;
    use windows_sys::Win32::Security::PROTECTED_DACL_SECURITY_INFORMATION;
    use windows_sys::Win32::Security::RevertToSelf;
    use windows_sys::Win32::Security::SecurityImpersonation;
    use windows_sys::Win32::Security::SetFileSecurityW;

    fn access_allowed(path: &std::path::Path, token: HANDLE, access: u32) -> Result<bool> {
        use windows_sys::Win32::Security::AccessCheck;
        use windows_sys::Win32::Security::GENERIC_MAPPING;
        use windows_sys::Win32::Security::GROUP_SECURITY_INFORMATION;
        use windows_sys::Win32::Security::GetFileSecurityW;
        use windows_sys::Win32::Security::OWNER_SECURITY_INFORMATION;
        use windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS;
        use windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_EXECUTE;
        use windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_READ;
        use windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_WRITE;

        let information =
            OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
        let mut needed = 0;
        unsafe {
            GetFileSecurityW(
                to_wide(path).as_ptr(),
                information,
                std::ptr::null_mut(),
                0,
                &mut needed,
            );
        }
        ensure!(needed > 0, "query test descriptor size");
        let mut descriptor = vec![0_u64; (needed as usize).div_ceil(8)];
        let mut privileges = [0_u64; 128];
        let mut privilege_size = std::mem::size_of_val(&privileges) as u32;
        let mapping = GENERIC_MAPPING {
            GenericRead: FILE_GENERIC_READ,
            GenericWrite: FILE_GENERIC_WRITE,
            GenericExecute: FILE_GENERIC_EXECUTE,
            GenericAll: FILE_ALL_ACCESS,
        };
        let mut granted = 0;
        let mut allowed = 0;
        unsafe {
            ensure!(
                GetFileSecurityW(
                    to_wide(path).as_ptr(),
                    information,
                    descriptor.as_mut_ptr().cast(),
                    needed,
                    &mut needed
                ) != 0
            );
            ensure!(
                AccessCheck(
                    descriptor.as_mut_ptr().cast(),
                    token,
                    access,
                    &mapping,
                    privileges.as_mut_ptr().cast(),
                    &mut privilege_size,
                    &mut granted,
                    &mut allowed
                ) != 0,
                "AccessCheck: {}",
                GetLastError()
            );
        }
        Ok(allowed != 0)
    }

    struct Impersonation;
    impl Drop for Impersonation {
        fn drop(&mut self) {
            assert_ne!(unsafe { RevertToSelf() }, 0, "restore test thread token");
        }
    }

    let fixture = tempfile::tempdir()?;
    unsafe {
        let mut descriptor = std::ptr::null_mut();
        ensure!(
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                to_wide("D:P(A;OICI;FA;;;WD)(A;OICI;GRGX;;;S-1-5-21-10-20-30-40)",).as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            ) != 0
        );
        let installed = SetFileSecurityW(
            to_wide(fixture.path()).as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        );
        LocalFree(descriptor as HLOCAL);
        ensure!(
            installed != 0,
            "install public fixture ACL: {}",
            GetLastError()
        );
    }
    let capability = LocalSid::from_string("S-1-5-21-10-20-30-40")?;
    let base = unsafe { OwnedHandle::from_raw_handle(get_current_token_for_restriction()?) };
    for create_token in [
        create_workspace_write_token_with_caps_from,
        create_readonly_token_with_caps_from,
    ] {
        let file = fixture.path().join("public.txt");
        let directory = fixture.path().join("public-directory");
        std::fs::write(&file, "readable")?;
        std::fs::create_dir(&directory)?;
        let token = unsafe {
            OwnedHandle::from_raw_handle(create_token(
                base.as_raw_handle(),
                &[capability.as_ptr()],
            )?)
        };
        let mut impersonation = std::ptr::null_mut();
        ensure!(
            unsafe {
                DuplicateToken(
                    token.as_raw_handle(),
                    SecurityImpersonation,
                    &mut impersonation,
                )
            } != 0
        );
        let impersonation = unsafe { OwnedHandle::from_raw_handle(impersonation) };
        // Compare the explicit access check with real NTFS operations. This
        // distinguishes token write checks from filesystem delete semantics.
        let checked_write = access_allowed(
            &file,
            impersonation.as_raw_handle(),
            windows_sys::Win32::Storage::FileSystem::FILE_WRITE_DATA,
        )?;
        let checked_delete = access_allowed(
            &file,
            impersonation.as_raw_handle(),
            windows_sys::Win32::Storage::FileSystem::DELETE,
        )?;
        ensure!(unsafe { ImpersonateLoggedOnUser(impersonation.as_raw_handle()) } != 0);
        let guard = Impersonation;
        let read = std::fs::read_to_string(&file);
        let write = std::fs::OpenOptions::new().write(true).open(&file);
        let delete_file = std::fs::remove_file(&file);
        let delete_directory = std::fs::remove_dir(&directory);
        drop(guard);

        assert_eq!(read?, "readable");
        assert!(
            write.is_err() && delete_file.is_err() && delete_directory.is_err(),
            "restricted-token mutations: write={write:?}, delete_file={delete_file:?}, delete_directory={delete_directory:?}; AccessCheck: write={checked_write}, delete={checked_delete}"
        );
        assert_eq!(std::fs::read_to_string(&file)?, "readable");
        assert!(directory.is_dir());
        std::fs::remove_dir(&directory)?;
    }
    Ok(())
}

#[test]
fn elevated_token_includes_network_proxy_restricting_sid() -> Result<()> {
    let capability_sid = LocalSid::from_string("S-1-5-21-10-20-30-40")?;
    let network_proxy_sid = LocalSid::from_string("S-1-5-21-50-60-70-80")?;
    let base_token = unsafe { get_current_token_for_restriction()? };
    let restricted_token = unsafe {
        create_readonly_token_with_caps_and_user_from(
            base_token,
            &[capability_sid.as_ptr()],
            &[network_proxy_sid.as_ptr()],
        )?
    };

    let has_network_proxy_sid =
        unsafe { token_has_restricting_sid(restricted_token, network_proxy_sid.as_ptr()) };
    unsafe {
        CloseHandle(restricted_token);
        CloseHandle(base_token);
    }

    assert!(has_network_proxy_sid?);
    Ok(())
}

#[test]
fn default_objects_deny_other_logons_even_with_the_same_owner() -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::io::FromRawHandle;
    use std::os::windows::io::OwnedHandle;
    use windows_sys::Win32::Security::ACCESS_ALLOWED_ACE;
    use windows_sys::Win32::Security::AccessCheck;
    use windows_sys::Win32::Security::DuplicateToken;
    use windows_sys::Win32::Security::GENERIC_MAPPING;
    use windows_sys::Win32::Security::GetAce;
    use windows_sys::Win32::Security::InitializeSecurityDescriptor;
    use windows_sys::Win32::Security::MapGenericMask;
    use windows_sys::Win32::Security::SECURITY_DESCRIPTOR;
    use windows_sys::Win32::Security::SecurityImpersonation;
    use windows_sys::Win32::Security::SetSecurityDescriptorDacl;
    use windows_sys::Win32::Security::SetSecurityDescriptorGroup;
    use windows_sys::Win32::Security::SetSecurityDescriptorOwner;
    use windows_sys::Win32::Storage::FileSystem::READ_CONTROL;
    use windows_sys::Win32::Storage::FileSystem::WRITE_DAC;
    use windows_sys::Win32::System::Threading::PROCESS_ALL_ACCESS;
    use windows_sys::Win32::System::Threading::PROCESS_CREATE_THREAD;
    use windows_sys::Win32::System::Threading::PROCESS_QUERY_INFORMATION;
    use windows_sys::Win32::System::Threading::PROCESS_TERMINATE;
    use windows_sys::Win32::System::Threading::PROCESS_VM_OPERATION;
    use windows_sys::Win32::System::Threading::PROCESS_VM_READ;
    use windows_sys::Win32::System::Threading::PROCESS_VM_WRITE;

    unsafe {
        let base = OwnedHandle::from_raw_handle(get_current_token_for_restriction()? as _);
        let base_raw = base.as_raw_handle() as HANDLE;
        let capability = LocalSid::from_string("S-1-5-21-10-20-30-40")?;
        let victim =
            OwnedHandle::from_raw_handle(create_workspace_write_token_with_caps_and_user_from(
                base_raw,
                &[capability.as_ptr()],
                &[],
            )? as _);
        let mut needed = 0;
        GetTokenInformation(
            victim.as_raw_handle() as _,
            TokenDefaultDacl,
            std::ptr::null_mut(),
            0,
            &mut needed,
        );
        let mut buffer = vec![0u8; needed as usize];
        ensure!(
            GetTokenInformation(
                victim.as_raw_handle() as _,
                TokenDefaultDacl,
                buffer.as_mut_ptr().cast(),
                needed,
                &mut needed
            ) != 0,
            "GetTokenInformation(TokenDefaultDacl) failed: {}",
            GetLastError()
        );
        let default_dacl = std::ptr::read_unaligned(buffer.as_ptr().cast::<TokenDefaultDaclInfo>());
        let mapping = GENERIC_MAPPING {
            GenericRead: READ_CONTROL | PROCESS_VM_READ | PROCESS_QUERY_INFORMATION,
            GenericWrite: READ_CONTROL | PROCESS_VM_WRITE | PROCESS_VM_OPERATION,
            GenericExecute: READ_CONTROL,
            GenericAll: PROCESS_ALL_ACCESS,
        };
        // Object creation maps the token default DACL's generic ACE masks to
        // object-specific rights. AccessCheck expects that mapping already done.
        for index in 0..u32::from((*default_dacl.default_dacl).AceCount) {
            let mut ace = std::ptr::null_mut();
            ensure!(GetAce(default_dacl.default_dacl, index, &mut ace) != 0);
            MapGenericMask(&mut (*ace.cast::<ACCESS_ALLOWED_ACE>()).Mask, &mapping);
        }
        let mut owner = get_user_sid_bytes(base_raw)?;
        let mut descriptor: SECURITY_DESCRIPTOR = std::mem::zeroed();
        let sd = std::ptr::addr_of_mut!(descriptor).cast();
        ensure!(InitializeSecurityDescriptor(sd, 1) != 0);
        ensure!(SetSecurityDescriptorOwner(sd, owner.as_mut_ptr().cast(), 0) != 0);
        ensure!(SetSecurityDescriptorGroup(sd, owner.as_mut_ptr().cast(), 0) != 0);
        ensure!(SetSecurityDescriptorDacl(sd, 1, default_dacl.default_dacl, 0) != 0);

        // Model another logon of the same account without the victim's logon
        // SID. Build from the base token, retaining the shared user, Everyone,
        // and filesystem capability in the write-restricting SID set.
        let mut logon = get_logon_sid_bytes(base_raw)?;
        let mut everyone = world_sid()?;
        let disabled = SID_AND_ATTRIBUTES {
            Sid: logon.as_mut_ptr().cast(),
            Attributes: 0,
        };
        let restricting = [
            capability.as_ptr(),
            owner.as_mut_ptr().cast(),
            everyone.as_mut_ptr().cast(),
        ]
        .map(|sid| SID_AND_ATTRIBUTES {
            Sid: sid,
            Attributes: 0,
        });
        let mut other = std::ptr::null_mut();
        ensure!(
            CreateRestrictedToken(
                base_raw,
                DISABLE_MAX_PRIVILEGE | LUA_TOKEN,
                1,
                &disabled,
                0,
                std::ptr::null(),
                restricting.len() as u32,
                restricting.as_ptr(),
                &mut other
            ) != 0,
            "CreateRestrictedToken(other logon) failed: {}",
            GetLastError()
        );
        let other = OwnedHandle::from_raw_handle(other as _);
        for (token, same_logon) in [(&victim, true), (&other, false)] {
            let mut impersonation = std::ptr::null_mut();
            ensure!(
                DuplicateToken(
                    token.as_raw_handle() as _,
                    SecurityImpersonation,
                    &mut impersonation
                ) != 0,
                "DuplicateToken failed: {}",
                GetLastError()
            );
            let impersonation = OwnedHandle::from_raw_handle(impersonation as _);
            for access in [
                PROCESS_VM_READ,
                PROCESS_VM_WRITE,
                PROCESS_VM_OPERATION,
                PROCESS_CREATE_THREAD,
                PROCESS_TERMINATE,
                PROCESS_QUERY_INFORMATION,
                WRITE_DAC,
                0x43a,
            ] {
                let mut privileges = [0u64; 128];
                let mut privilege_bytes = std::mem::size_of_val(&privileges) as u32;
                let mut granted = 0;
                let mut allowed = 0;
                ensure!(
                    AccessCheck(
                        sd,
                        impersonation.as_raw_handle() as _,
                        access,
                        &mapping,
                        privileges.as_mut_ptr().cast(),
                        &mut privilege_bytes,
                        &mut granted,
                        &mut allowed
                    ) != 0,
                    "AccessCheck failed: {}",
                    GetLastError()
                );
                assert_eq!(allowed != 0, same_logon, "access {access:#x}");
            }
        }
    }
    Ok(())
}
