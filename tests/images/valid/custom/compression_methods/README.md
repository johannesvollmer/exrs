This directory contains the same images, 
compressed with different compression methods, 
which will be compared with the uncompressed data.

Furthermore, it contains `uncompressed_xxx` images. 
For example, the `decompressed_b44` image is uncompressed, 
but contains pixels that have been compressed with `b44` once, losing precision.
This is used to compare the changes happening when compressing with a lossy method.