OpenEXR-images
==============

This repository contains test images for the OpenEXR-2.0 library.
We have provided three different versions of a single scene split into four separate passes, as well as the combined result as a regular image for comparison.

Since the repository is quite large, consider downloading only the individual images required, rather than cloning it using git.

In each case, Leaves.exr, Trunk.exr, Ground.exr and Balls.exr are individual deep passes.

Stereo:
=======
left and right views of each image, at 1920x1080 (Full HD) resolution. These files are large!
This folder also contains a composited, flattened, image, with separate views and depth channel, as a regular "scanlineimage" EXR.
The composited image will only be viewable in packages compiled with OpenEXR-2.0 or later, as it has four separate parts

LeftView:
=========
Only the left view of each image, at 1920x1080 (Full HD) resolution.


LowResLeftView:
===============
Only the left view, downsampled by decimation to 1024x576 (there is a visible shift in the image compared to the 1920x1080 images)
This folder also contains a composited, flattened image, with no depth channel, as a regular "scanlineimage" EXR.
This image will be viewable in packages compiled with older OpenEXR libraries, since it does not rely on any OpenEXR-2.0 specific features.


All images (c) 2012 Weta Digital Ltd

