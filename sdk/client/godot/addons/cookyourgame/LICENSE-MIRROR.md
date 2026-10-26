# Mirror attribution

The transport and metrics layer of this SDK (`addons/cookyourgame/core/` and
`addons/cookyourgame/profiler/`) follows the published design of **Mirror**, the
Unity networking library: interest management, transport abstraction and the
per-opcode byte/packet counters used by the profiler.

That design is studied from Mirror's documentation and public source, and the
code is an independent reimplementation written against this project's own wire
protocol. No upstream source text is copied. Mirror is MIT licensed, so the
upstream notice is reproduced here and must accompany any redistribution of the
plugin.

---

MIT License

Copyright (c) 2015, Unity Technologies
Copyright (c) 2019, Mischa, Paul, Chris, Robin, Stephen and Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
