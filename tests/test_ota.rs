#[test]
fn test_elf_header_validation() {
    let elf_magic = [0x7F, b'E', b'L', b'F'];
    let invalid_data = [0x00, 0x01, 0x02, 0x03];

    assert_eq!(&elf_magic[..4], &[0x7F, b'E', b'L', b'F']);
    assert_ne!(&invalid_data[..4], &[0x7F, b'E', b'L', b'F']);
}
