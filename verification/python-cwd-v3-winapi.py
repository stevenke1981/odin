"""Fixed, read-only WinAPI cwd/access-mask observation for the native probe."""
import ctypes
from ctypes import wintypes
import json
import sys

kernel = ctypes.WinDLL('kernel32', use_last_error=True)
kernel.GetCurrentDirectoryW.argtypes = [wintypes.DWORD, wintypes.LPWSTR]
kernel.GetCurrentDirectoryW.restype = wintypes.DWORD
kernel.SetCurrentDirectoryW.argtypes = [wintypes.LPCWSTR]
kernel.SetCurrentDirectoryW.restype = wintypes.BOOL
kernel.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                              wintypes.LPVOID, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
kernel.CreateFileW.restype = wintypes.HANDLE
kernel.CloseHandle.argtypes = [wintypes.HANDLE]
kernel.CloseHandle.restype = wintypes.BOOL


def cwd():
    buffer = ctypes.create_unicode_buffer(32768)
    ctypes.set_last_error(0)
    length = kernel.GetCurrentDirectoryW(len(buffer), buffer)
    error = ctypes.get_last_error()
    return {'cwd': buffer.value if length else None, 'length': length,
            'last_error': error, 'last_error_valid': not bool(length)}


def access(path, mask):
    ctypes.set_last_error(0)
    handle = kernel.CreateFileW(path, mask, 7, None, 3, 0x02000000, None)
    error = ctypes.get_last_error()
    success = handle != ctypes.c_void_p(-1).value
    if success:
        kernel.CloseHandle(handle)
    return {'mask': mask, 'success': success, 'last_error': error,
            'last_error_valid': not success}


if sys.argv[1:] == ['--cwd-only']:
    print(json.dumps(cwd()))
else:
    with open(sys.argv[1], encoding='utf-8') as stream:
        data = json.load(stream)
    result = {'initial': cwd(), 'targets': []}
    for item in data['targets']:
        attempt = {**item, 'before': cwd(), 'access_masks': []}
        for mask in [0, 0x80, 0x20, 0x100000, 0x100020, 0x100080]:
            attempt['access_masks'].append(access(item['path'], mask))
        ctypes.set_last_error(0)
        success = bool(kernel.SetCurrentDirectoryW(item['path']))
        error = ctypes.get_last_error()
        attempt.update(success=success, last_error=error, last_error_valid=not success, after=cwd())
        result['targets'].append(attempt)
    print(json.dumps(result))
