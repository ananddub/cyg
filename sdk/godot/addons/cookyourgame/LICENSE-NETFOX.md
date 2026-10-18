# Netfox attribution

The rollback layer of this SDK (`addons/cookyourgame/rollback/`, and the
prediction/interpolation behaviour of `addons/cookyourgame/components/`) follows
the published design of **Netfox**, the Godot 4 network synchronization system:
client-side prediction with a per-tick history buffer, server reconciliation
with rollback and replay, and entity interpolation between snapshots.

That design is studied from Netfox's documentation and public source, and the
code is written against this project's own wire protocol. It is still a
derivative of Netfox's work, which is MIT licensed, so the upstream notice is
reproduced here and must accompany any redistribution of the plugin.

---

Copyright 2023 Gálffy Tamás

Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files (the "Software"), to deal in
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software is furnished to do so,
subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
