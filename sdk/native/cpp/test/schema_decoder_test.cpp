// Schema decoder smoke test. No generated bindings required: this file
// defines a generated-style entity and builds frames by hand.
//
//   g++ -std=c++17 -I sdk/native/cpp sdk/native/cpp/test/schema_decoder_test.cpp
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <optional>
#include <string>
#include <vector>

#include "openoxide_schema_runtime.hpp"

using namespace openoxide::schema;

namespace {

constexpr uint32_t kPlayerHash = 0x11111111u;
constexpr uint32_t kRootHash = 0x22222222u;

struct Player final : Entity {
    inline static constexpr uint32_t kHash = kPlayerHash;
    inline static const std::vector<FieldDef> kFields = {
        {"x", WireKind::F64, 0, WireKind::U8, 8},
        {"hp", WireKind::U8, 0, WireKind::U8, 1},
        {"tags", WireKind::Array, 0, WireKind::U32, 0},
        {"target", WireKind::U32, FieldDef::kOptional, WireKind::U8, 4},
    };

    double x = 0.0;
    uint8_t hp = 0;
    std::vector<uint32_t> tags;
    std::optional<uint32_t> target;

    int field_count() const override { return static_cast<int>(kFields.size()); }

    void set_field(int field, const Value& value) override {
        switch (field) {
            case 0: x = value.as_f64(); break;
            case 1: hp = value.as_u8(); break;
            case 2: {
                tags.clear();
                for (const auto& item : value.as_array()) tags.push_back(item.as_u32());
                break;
            }
            case 3: target = value.is_null() ? std::nullopt : std::optional<uint32_t>(value.as_u32()); break;
            default: break;
        }
    }

    Value get_field(int field) const override {
        switch (field) {
            case 0: return Value::make_f64(x);
            case 1: return Value::make_u8(hp);
            case 2: {
                std::vector<Value> items;
                for (uint32_t tag : tags) items.push_back(Value::make_u32(tag));
                return Value::make_array(std::move(items));
            }
            case 3: return target ? Value::make_u32(*target) : Value::null();
            default: return Value::null();
        }
    }

    void clear_fields() override {
        x = 0.0;
        hp = 0;
        tags.clear();
        target = std::nullopt;
    }
};

int checks = 0;
bool failed = false;

void check(const char* name, bool ok) {
    checks++;
    if (!ok) failed = true;
    std::printf("%s %s\n", ok ? "CHECK" : "FAIL ", name);
}

struct Buf {
    std::vector<uint8_t> bytes;

    void u8(uint8_t v) { bytes.push_back(v); }
    void u16(uint16_t v) { u8(uint8_t(v)); u8(uint8_t(v >> 8)); }
    void u32(uint32_t v) { u16(uint16_t(v)); u16(uint16_t(v >> 16)); }
    void i64(int64_t v) { u32(uint32_t(uint64_t(v))); u32(uint32_t(uint64_t(v) >> 32)); }
    void f32(float v) { uint32_t bits; std::memcpy(&bits, &v, 4); u32(bits); }
    void f64(double v) { uint64_t bits; std::memcpy(&bits, &v, 8); i64(int64_t(bits)); }
    void s16(const std::string& v) { u16(uint16_t(v.size())); bytes.insert(bytes.end(), v.begin(), v.end()); }
    void s8(const std::string& v) { u8(uint8_t(v.size())); bytes.insert(bytes.end(), v.begin(), v.end()); }
    void magic() { bytes.insert(bytes.end(), {'O', 'X', 'S', '2'}); }
};

void field(Buf& b, const char* name, uint8_t code, uint8_t flags, uint8_t element) {
    b.s16(name);
    b.u8(code);
    b.u8(flags);
    b.u8(element);
}

std::vector<uint8_t> descriptor() {
    Buf b;
    b.bytes.insert(b.bytes.end(), {'O', 'X', 'S', 'D'});
    b.u8(3);
    b.u8(0);
    b.u16(1);
    b.s16("Player");
    b.u32(1);
    b.u32(kPlayerHash);
    b.u16(4);
    field(b, "x", 6, 0, 0);
    field(b, "hp", 1, 0, 0);
    field(b, "tags", 12, 0, 3);
    field(b, "target", 3, 1, 0);
    b.s16("State");
    b.u32(1);
    b.u32(kRootHash);
    b.u16(1);
    field(b, "round", 2, 0, 0);
    b.u16(1);
    b.s16("players");
    b.u16(0);
    return b.bytes;
}

void row(Buf& s, uint32_t id, double x, uint8_t hp, const std::vector<uint32_t>& tags,
         bool has_target, uint32_t target) {
    s.u32(id);
    s.f64(x);
    s.u8(hp);
    s.u8(uint8_t(tags.size()));
    for (uint32_t tag : tags) s.u32(tag);
    s.u8(has_target ? 1 : 0);
    if (has_target) s.u32(target);
}

std::vector<uint8_t> section_full() {
    Buf s;
    s.magic();
    s.u8(1);
    s.u32(kPlayerHash);
    s.u32(2);
    row(s, 1, 1.5, 10, {7, 9}, true, 42);
    row(s, 2, 3.5, 20, {}, false, 0);
    return std::move(s.bytes);
}

void world_collection(Buf& f, uint16_t id, uint32_t hash, const std::vector<uint8_t>& section) {
    f.u16(id);
    f.u32(hash);
    f.u32(uint32_t(section.size()));
    f.bytes.insert(f.bytes.end(), section.begin(), section.end());
}

std::vector<uint8_t> world_full() {
    Buf f;
    f.magic();
    f.u8(3);
    f.u32(kRootHash);
    f.u16(1);
    f.u16(7);
    f.u16(1);
    world_collection(f, 0, kPlayerHash, section_full());
    return std::move(f.bytes);
}

std::vector<uint8_t> world_patch() {
    Buf f;
    f.magic();
    f.u8(4);
    f.u32(kRootHash);
    f.u16(1);
    f.u32(1);
    f.u16(8);

    Buf p;
    p.magic();
    p.u8(2);
    p.u32(kPlayerHash);
    p.u32(2);
    p.u8(3);
    p.u32(1);
    p.u32((1u << 0) | (1u << 3));
    p.f64(9.5);
    p.u8(0);
    p.u8(2);
    p.u32(2);
    f.u16(1);
    world_collection(f, 0, kPlayerHash, p.bytes);
    return std::move(f.bytes);
}

}  // namespace

int main() {
    Registry registry;
    registry.add("Player", []() -> std::unique_ptr<Entity> { return std::make_unique<Player>(); });

    auto state = State::from_descriptor(descriptor(), &registry);
    check("descriptor parses", state.has_value());
    check("bad descriptor rejected", !State::from_descriptor(std::vector<uint8_t>{'O', 'X', 'S', 'D', 9}));
    if (!state) return 1;

    int adds = 0, removes = 0, changes = 0, updates = 0, root_changes = 0;
    state->on_entity_added = [&](const std::string&, uint32_t, Entity&) { adds++; };
    state->on_entity_removed = [&](const std::string&, uint32_t, Entity&) { removes++; };
    state->on_field_changed = [&](const std::string&, uint32_t, int, const Value&, const Value&) {
        changes++;
    };
    state->on_entity_updated = [&](const std::string&, uint32_t, Entity&) { updates++; };
    state->on_root_field_changed = [&](int, const Value&, const Value&) { root_changes++; };

    check("world full applies", state->apply(world_full()));
    check("ready", state->ready());
    check("added two", adds == 2 && state->collection("players")->entities.size() == 2);

    auto* p1 = dynamic_cast<Player*>(state->collection("players")->find(1));
    auto* p2 = dynamic_cast<Player*>(state->collection("players")->find(2));
    check("typed row", p1 && p1->x == 1.5 && p1->hp == 10);
    check("array field", p1 && p1->tags.size() == 2 && p1->tags[1] == 9);
    check("optional present", p1 && p1->target.has_value() && *p1->target == 42);
    check("optional absent", p2 && !p2->target.has_value() && p2->tags.empty());
    check("root scalar", state->root_entity().get_field(0).as_u16() == 7);

    check("world patch applies", state->apply(world_patch()));
    check("root patched", state->root_entity().get_field(0).as_u16() == 8);
    check("field updated", p1 && p1->x == 9.5 && !p1->target.has_value());
    check("entity removed", state->collection("players")->find(2) == nullptr);
    check("events", removes == 1 && changes == 2 && updates == 1 && root_changes == 2);

    auto bare = State::from_descriptor(descriptor(), &registry);
    check("patch before full dropped", bare && !bare->apply(world_patch()));
    check("truncated rejected", !state->apply(std::vector<uint8_t>{'O', 'X', 'S', '2', 1, 0}));
    check("foreign rejected", !state->apply(std::vector<uint8_t>{1, 2, 3, 4, 5}));

    std::printf("checks=%d\n", checks);
    return failed ? 1 : 0;
}
