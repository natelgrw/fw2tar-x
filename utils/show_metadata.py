#!/usr/bin/env python3
import argparse
import gzip
import json

def print_base_metadata(metadata):
    print("Made with fw2tar")
    print(f"  File: {metadata.get('file', '?')}")
    print(f"  Generated with command: {metadata.get('fw2tar_command', '?')}")
    print(f"  Input file SHA1: {metadata.get('input_hash', '?')}")

def print_firmware_metadata(metadata):
    print(f"Firmware Profile: {metadata.get('file', '?')}")
    print(f"  Input file SHA1: {metadata.get('input_hash', '?')}")
    print(f"  Image Size:      {metadata.get('image_size', 0)} bytes")
    print(f"  Command:         {' '.join(metadata.get('fw2tar_command', []))}")
    
    archives = metadata.get("archives", [])
    if not archives:
        print("\n  No archives extracted.")
        return
        
    print(f"\n  Extracted Archives ({len(archives)}):")
    for arc in archives:
        print(f"    - Path:        {arc.get('path', '?')}")
        print(f"      Extractor:   {arc.get('extractor', '?')}")
        print(f"      Score:       {arc.get('rootfs_score', 0):.2f}")
        print(f"      Nodes:       {arc.get('file_node_count', 0)}")
        print(f"      Merged:      {arc.get('was_merged', False)}")
        print(f"      Archive Hsh: {arc.get('archive_hash', '?')}")
        print()

def main(firmware):
    if firmware.endswith('.json'):
        with open(firmware, 'r') as f:
            metadata = json.load(f)
        print_firmware_metadata(metadata)
        return

    with gzip.open(firmware, 'rb') as f:
        f.seek(-0x1000, 2)
        end_bytes = f.read()

    str_len = list(end_bytes[::-1]).index(0)
    string = end_bytes[-str_len:].decode()

    metadata, magic = string.split('\n')
    assert(magic == "made with fw2tar")

    metadata = json.loads(metadata)
    print_base_metadata(metadata)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Show metadata from fw2tar output archive or JSON")
    parser.add_argument("firmware", type=str, help="Output .tar.gz or .json from fw2tar")

    args = parser.parse_args()

    main(args.firmware)
