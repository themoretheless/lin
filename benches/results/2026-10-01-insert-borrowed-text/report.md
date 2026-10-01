# Borrowed embedding text follow-up

One process, eight observations per case; background load uncontrolled.
Full 10k insertion: 18.447 ms; no embedding: 11.526 ms; no embedding or
scalar index: 9.894 ms; neither those nor FTS: 7.985 ms.
This diagnostic run has no paired baseline and proves no speedup or peer win.
The change removes text allocation for a single effective embedding field;
multi-field concatenation preserves the original separators.
