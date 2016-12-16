# pegasus
[![Build Status](https://travis-ci.org/kvark/pegasus.svg)](https://travis-ci.org/kvark/pegasus)
[![Crates.io](https://img.shields.io/crates/v/pegasus.svg?maxAge=2592000)](https://crates.io/crates/pegasus)

GFX + Specs framework that lets you fly

GFX is made for the command buffers to be constructed in parallel.
Specs is made to do operations on a large number of entities in parallel.
So what about using them together?

Pegasus comes to help, serving both as an example of how to organize the system flow, and as a framework.
It is most useful to users who don't want to deal with multiple command buffers being passed between threads, and just want to draw stuff while still enjoying all the benefits of a parallel ECS.

Featured users:
 - [yasteroids](https://github.com/kvark/yasteroids)
