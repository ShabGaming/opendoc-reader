import errno
import os

import pytest

from opendoc_engine.exporters import files
from opendoc_engine.exporters.files import OutputError


def test_write_new_creates_parent_folders_and_leaves_no_temporary_file(tmp_path):
    target = tmp_path / "a" / "b" / "out.bin"
    files.write_new(target, b"data")
    assert target.read_bytes() == b"data"
    assert [p.name for p in target.parent.iterdir()] == ["out.bin"]


def test_write_new_refuses_to_replace(tmp_path):
    target = tmp_path / "out.bin"
    target.write_bytes(b"first")
    with pytest.raises(OutputError) as raised:
        files.write_new(target, b"second")
    assert raised.value.reason == "already_exists"
    assert target.read_bytes() == b"first"
    assert [p.name for p in tmp_path.iterdir()] == ["out.bin"]


def test_a_file_that_appears_during_the_write_is_not_replaced(tmp_path, monkeypatch):
    target = tmp_path / "out.bin"
    original = files._write_temp

    def racing(dest, data, source):
        tmp = original(dest, data, source)
        dest.write_bytes(b"winner")  # someone else takes the name after our check
        return tmp

    monkeypatch.setattr(files, "_write_temp", racing)
    with pytest.raises(OutputError) as raised:
        files.write_new(target, b"loser")
    assert raised.value.reason == "already_exists"
    assert target.read_bytes() == b"winner"
    assert [p.name for p in tmp_path.iterdir()] == ["out.bin"]


def test_copy_new_copies_and_refuses_to_replace(tmp_path):
    source = tmp_path / "src.png"
    source.write_bytes(b"png")
    target = tmp_path / "assets" / "src.png"
    files.copy_new(source, target)
    assert target.read_bytes() == b"png"
    with pytest.raises(OutputError):
        files.copy_new(source, target)


def test_write_replacing_replaces(tmp_path):
    target = tmp_path / "doc.json"
    target.write_bytes(b"old")
    files.write_replacing(target, b"new")
    assert target.read_bytes() == b"new"


def test_os_errors_map_to_protocol_reasons(tmp_path, monkeypatch):
    def failing(code):
        def raise_it(*_args, **_kwargs):
            raise OSError(code, os.strerror(code))

        return raise_it

    monkeypatch.setattr(files, "_write_temp", failing(errno.ENOSPC))
    with pytest.raises(OutputError) as full:
        files.write_new(tmp_path / "a", b"x")
    assert full.value.reason == "disk_full"

    monkeypatch.setattr(files, "_write_temp", failing(errno.EIO))
    with pytest.raises(OutputError) as other:
        files.write_new(tmp_path / "b", b"x")
    assert other.value.reason == "write_failed"

    def denied(*_args, **_kwargs):
        raise PermissionError(errno.EACCES, "denied")

    monkeypatch.setattr(files, "_write_temp", denied)
    with pytest.raises(OutputError) as refused:
        files.write_new(tmp_path / "c", b"x")
    assert refused.value.reason == "permission_denied"
