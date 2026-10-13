# Independent clip-angle references

Python standard `math.pi` calculated `clip-masses.json` outside the Rust geometry/model code. Units mm, mm3, kg; steel density 7850kg/m3.

- I300 primary: area 2*150*12 + 8*(300-24) = 5808mm2; length3000. Remove two holes of radius11 through an8mm web: 2*pi*121*8.
- L100x100x20: area 2*100*20-20^2=3600mm2; length170. Remove four holes of radius11 through20mm legs: 4*pi*121*20.
- Support plate600x600x30: remove two holes of radius11 through30mm: 2*pi*121*30.
- Mass = net volume *7850/1e9.

The test integrates signed tetrahedral volume of the actual f64 mesh separately, checks both endpoints, and checks model mass against these reference values at1e-6 relative tolerance. Additional I320x160x10x14 and angle-length190 references use analytic areas7400 and3600mm2. Sharp corners are used consistently; these fixtures do not establish curved-support, rolled-fillet or capacity calculations.
