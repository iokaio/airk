<?php
// SPDX-License-Identifier: Apache-2.0
namespace Fictional\Archive;
use Fictional\Shared\Reader as LocalReader;
trait Named { public function label() { return "café"; } }
interface Readable { public function read(); }
class Archive implements Readable {
    use Named;
    public function read() {
        $reader->open();
        $reader?->close();
        LocalReader::create();
        $reader->$method();
    }
}
