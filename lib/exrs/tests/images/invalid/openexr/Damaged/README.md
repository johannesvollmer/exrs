# Damaged Images

The images included here are ones which have previously caused issues with the OpenEXR library, such as out-of-bounds reads or writes, or caused the OpenEXR library to leak memory when handling the error.

Most of these images were made by 'fuzzing' existing images, by modifying or rearranging bytes within the file, or truncating files early. As such, they are **not valid OpenEXR images** so are not stored here with a ".exr" extension.
In most cases, the correct behavior of the library is to detect the broken file and throw an exception.

These files have been archived here to permit regression testing, ensuring that future modifications to the library do not inadvertently reintroduce a bug that previously existed. Testing generally requires use of an analysis tool such as the [LLVM sanitizer tool](https://github.com/google/sanitizers/) or [valgrind](https://valgrind.org/).
In unpatched OpenEXR libraries, the analyzer may report errors when running standard tools `exrheader`, `exrmakepreview` or `exrmaketiled` with these images, or the tool may crash with a segmentation fault or similar error. Note that some of these images may cause excessive memory to be allocated, or long run times. This is not considered a bug, but some analyzers will report this behavior as an error.