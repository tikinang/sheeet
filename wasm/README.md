
# How does the algorithm work?
## Load from serialized data
1. set all cells `raw_value` and parse `expression`
2. iterate cells and compute `cached_value`, recursively for all dependencies from `expression`
   OR
   save even `cached_value` and then it should be enough to just load it

## Update cell value

If A1 depends on A2 and A3 (are in expression as reference),

```
A1=add(A2,A3)

A1 -> [ A2, A3 ]
```

then A1 is dependent of both the A2 and A3,

```
A2 <- [ A1 ], A3 <- [ A1 ]
```

and when A2 or A3 is changed, the A1 should be recomputed.

That means, when I update `A2`:
- update `A2` cell raw value and parse new `expression`
- all dependencies from the new `expression` must be registered as dependents in the reversed index
- all dependencies from the old `expression` must be removed from the reversed dependents index
- and then all dependent cells' `cached_value` must be recomputed, recursively

```
A1=1
A2=2
A3=3
B1=A1+A2
B2=B1

B2 -> B1 -> [ A1, A2 ]
A1 <- [ B1 ], A2 <- [ B1 ], B1 <- [ B2 ]

# update A1 value must update B1 and B2
1. update A1
2. find B1 in reverse index (of A1)
3. update B1
4. find B2 in reverse index (of B1)
5. update B2

steps 2-5 are done recursively 

# update of B1=A2+A3 must update reverse index (add B1 to A3 and remove B1 from A1)
1. update B1
2. remove B1 from A1 reverse index
3. add B1 to A3 reverse index
4: find B2 in reverse index (of B1)
5. update B2

steps 1-3 are done once
steps 4-5 are done recursively
```
