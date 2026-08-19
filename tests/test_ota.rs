use arcadematrix::api::ota::validate_firmware;

#[test]
fn test_ota_validation_too_small() {
    let firmware_bytes = vec![0x7F, b'E', b'L', b'F']; // Only 4 bytes
    let result = validate_firmware(&firmware_bytes, "aarch64-unknown-linux-gnu");
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "Uploaded file too small to be a valid firmware binary"
    );
}

#[test]
fn test_ota_validation_missing_magic() {
    let mut firmware_bytes = vec![0; 20];
    firmware_bytes[0] = 0x00; // Invalid magic
    let result = validate_firmware(&firmware_bytes, "aarch64-unknown-linux-gnu");
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "Invalid firmware: missing ELF magic header"
    );
}

#[test]
fn test_ota_validation_valid_aarch64() {
    let mut firmware_bytes = vec![0; 20];
    firmware_bytes[0] = 0x7F;
    firmware_bytes[1] = b'E';
    firmware_bytes[2] = b'L';
    firmware_bytes[3] = b'F';

    // Set e_machine to EM_AARCH64 (183 / 0xB7)
    let e_machine_bytes = 183u16.to_le_bytes();
    firmware_bytes[18] = e_machine_bytes[0];
    firmware_bytes[19] = e_machine_bytes[1];

    let result = validate_firmware(&firmware_bytes, "aarch64-unknown-linux-gnu");
    assert!(result.is_ok());
}

#[test]
fn test_ota_validation_valid_arm() {
    let mut firmware_bytes = vec![0; 20];
    firmware_bytes[0] = 0x7F;
    firmware_bytes[1] = b'E';
    firmware_bytes[2] = b'L';
    firmware_bytes[3] = b'F';

    // Set e_machine to EM_ARM (40 / 0x28)
    let e_machine_bytes = 40u16.to_le_bytes();
    firmware_bytes[18] = e_machine_bytes[0];
    firmware_bytes[19] = e_machine_bytes[1];

    let result = validate_firmware(&firmware_bytes, "armv7-unknown-linux-gnueabihf");
    assert!(result.is_ok());
}

#[test]
fn test_ota_validation_arch_mismatch() {
    let mut firmware_bytes = vec![0; 20];
    firmware_bytes[0] = 0x7F;
    firmware_bytes[1] = b'E';
    firmware_bytes[2] = b'L';
    firmware_bytes[3] = b'F';

    // Set e_machine to EM_ARM (40 / 0x28)
    let e_machine_bytes = 40u16.to_le_bytes();
    firmware_bytes[18] = e_machine_bytes[0];
    firmware_bytes[19] = e_machine_bytes[1];

    // But try to install it on aarch64
    let result = validate_firmware(&firmware_bytes, "aarch64-unknown-linux-gnu");
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "Architecture mismatch: firmware is for e_machine 40, expected target aarch64-unknown-linux-gnu"
    );
}

#[test]
fn test_ota_validation_unknown_arch() {
    let mut firmware_bytes = vec![0; 20];
    firmware_bytes[0] = 0x7F;
    firmware_bytes[1] = b'E';
    firmware_bytes[2] = b'L';
    firmware_bytes[3] = b'F';

    // Set e_machine to EM_X86_64 (62 / 0x3E)
    let e_machine_bytes = 62u16.to_le_bytes();
    firmware_bytes[18] = e_machine_bytes[0];
    firmware_bytes[19] = e_machine_bytes[1];

    let result = validate_firmware(&firmware_bytes, "aarch64-unknown-linux-gnu");
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "Architecture mismatch: firmware is for e_machine 62, expected target aarch64-unknown-linux-gnu"
    );
}
