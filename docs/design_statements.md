# Statements for designing fast, efficient, or correct domain-definable systems

- Unknowns can be implied, implieds can be forgotten, forgottens risk harm.
- Dynamics risk unknowns.
- Every action is lossy.
- Slow can be complex, complex can be efficient, efficient can be simple, simple can be slow.
- Process and processee are 2 halves of one whole, how both divide can be influenced.
- Processes and processees can be fast and efficient through cheaper links.

## Why they are written down here

The first one is about itself. A rule that lives only in the head of
whoever wrote the code is implied; an implied rule is forgotten as soon
as somebody else reads the code, or as soon as enough time passes; and
what gets forgotten is what gets broken. So the statements are in the
repository, where the code that follows them is.

## How they have been read, in this repository

Not as slogans. Each one has decided something, and these are the
decisions, so that a later reader can tell whether a statement was
applied or just cited.

**Unknowns can be implied, implieds can be forgotten, forgottens risk
harm.** This is most of the naming. A name that leaves its meaning to
be inferred is an implied. `same` and `held` became
`homogeneous_tiles` and `homogeneous_tile_values`; `AT` became
`PYRAMID_LEVEL_BOUNDARIES`; `bits_of_its_own` became
`description_size`. It is also why the seed lives in a file rather than
a constant, why a run says when it is reusing one, and why the
encoding's costs are counted twice by different code and checked
against each other -- an accounting that only one piece of code knows
is an implied.

**Dynamics risk unknowns.** Why there are no caps and no tuned
constants in the encoding. A cap is a number that was right for the
bitmaps somebody tried; on the next bitmap it is an unknown. The tile
size field is as wide as its region's own size needs rather than a flat
three bits, because a flat field is a cap wearing a width.

**Every action is lossy.** Why every experiment decodes what it encoded
and compares cell for cell before reading a number off it, and why the
round trip is the first test and not the last. It is also the reason
masking is announced rather than derived: a description that leaves
something out has to say so, or the leaving out is the loss.

**Slow can be complex, complex can be efficient, efficient can be
simple, simple can be slow.** Why the encoder is written plainly, a
cell at a time, with nothing in it for speed. And why `region` and
`stream` are single files while `dsrn` is eight: a folder for two small
files is complexity that buys nothing.

**Process and processee are 2 halves of one whole, how both divide can
be influenced.** The encoder and the bitmap divide together. The
pyramid is that division made explicit -- it is not part of the
encoding, it is the shape of the question the encoding asks, and it is
useful on its own.

**Processes and processees can be fast and efficient through cheaper
links.** The link is what one part of the encoding has to say to
another. Making the mask a code rather than a flag on every binding was
exactly this: the same information, paid for only where it is used.
