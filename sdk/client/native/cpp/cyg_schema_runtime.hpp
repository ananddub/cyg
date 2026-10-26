// CYG schema runtime (C++17, header-only).
//
// Decodes the `OXSD` descriptor and applies `OXS2` full/patch sections. The
// generated bindings (`cyg_schema_generated.hpp`) subclass `Entity` and
// register typed factories with `Registry`.
//
//   Registry registry;
//   cyg::schema::generated::register_generated(registry);
//   auto state = State::from_descriptor(descriptor_bytes, &registry);
//   state->apply(section_bytes);
#pragma once

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <functional>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <utility>
#include <vector>

namespace cyg::schema {

/// Wire type codes shared by `OXSD` and `OXS2`.
enum class WireKind : uint8_t {
    None = 0,
    U8 = 1,
    U16 = 2,
    U32 = 3,
    I32 = 4,
    F32 = 5,
    F64 = 6,
    Bool = 7,
    I64 = 8,
    I8 = 9,
    I16 = 10,
    String = 11,
    Array = 12,
};

inline bool wire_kind_from_code(uint8_t code, WireKind& kind) {
    if (code < 1 || code > 12) return false;
    kind = static_cast<WireKind>(code);
    return true;
}

inline int wire_kind_size(WireKind kind) {
    switch (kind) {
        case WireKind::U8:
        case WireKind::I8:
        case WireKind::Bool:
            return 1;
        case WireKind::U16:
        case WireKind::I16:
            return 2;
        case WireKind::U32:
        case WireKind::I32:
        case WireKind::F32:
            return 4;
        case WireKind::F64:
        case WireKind::I64:
            return 8;
        default:
            return 0;
    }
}

/// One field of a schema, in wire order.
struct FieldDef {
    static constexpr uint8_t kOptional = 1;
    static constexpr uint8_t kStatic = 2;
    static constexpr uint8_t kTransient = 4;

    std::string name;
    WireKind kind = WireKind::None;
    uint8_t flags = 0;
    WireKind element = WireKind::None;
    int size = 0;

    bool optional() const { return (flags & kOptional) != 0; }
    bool full_only() const { return (flags & (kStatic | kTransient)) != 0; }
};

/// A decoded scalar, string, array or null.
class Value {
public:
    using Array = std::vector<Value>;

    Value() = default;

    static Value null() { return Value(); }

    static Value make_u8(uint8_t v) { return number(WireKind::U8, v); }
    static Value make_i8(int8_t v) { return number(WireKind::I8, v); }
    static Value make_u16(uint16_t v) { return number(WireKind::U16, v); }
    static Value make_i16(int16_t v) { return number(WireKind::I16, v); }
    static Value make_u32(uint32_t v) { return number(WireKind::U32, v); }
    static Value make_i32(int32_t v) { return number(WireKind::I32, v); }
    static Value make_f32(float v) {
        Value value;
        value.kind_ = WireKind::F32;
        value.number_ = static_cast<double>(v);
        return value;
    }
    static Value make_f64(double v) {
        Value value;
        value.kind_ = WireKind::F64;
        value.number_ = v;
        return value;
    }
    static Value make_bool(bool v) {
        Value value;
        value.kind_ = WireKind::Bool;
        value.boolean_ = v;
        return value;
    }
    static Value make_i64(int64_t v) {
        Value value;
        value.kind_ = WireKind::I64;
        value.integer_ = v;
        return value;
    }
    static Value make_string(std::string v) {
        Value value;
        value.kind_ = WireKind::String;
        value.string_ = std::move(v);
        return value;
    }
    static Value make_string(const char* v) { return make_string(std::string(v ? v : "")); }
    static Value make_array(Array v) {
        Value value;
        value.kind_ = WireKind::Array;
        value.array_ = std::move(v);
        return value;
    }

    WireKind kind() const { return kind_; }
    bool is_null() const { return kind_ == WireKind::None; }

    double as_f64() const { return kind_ == WireKind::I64 ? static_cast<double>(integer_) : number_; }
    float as_f32() const { return static_cast<float>(as_f64()); }
    uint8_t as_u8() const { return static_cast<uint8_t>(as_i64()); }
    int8_t as_i8() const { return static_cast<int8_t>(as_i64()); }
    uint16_t as_u16() const { return static_cast<uint16_t>(as_i64()); }
    int16_t as_i16() const { return static_cast<int16_t>(as_i64()); }
    uint32_t as_u32() const { return static_cast<uint32_t>(as_i64()); }
    int32_t as_i32() const { return static_cast<int32_t>(as_i64()); }
    int64_t as_i64() const {
        if (kind_ == WireKind::I64) return integer_;
        if (kind_ == WireKind::Bool) return boolean_ ? 1 : 0;
        return static_cast<int64_t>(number_);
    }
    bool as_bool() const { return kind_ == WireKind::Bool ? boolean_ : as_i64() != 0; }

    const std::string& as_string() const {
        static const std::string empty;
        return kind_ == WireKind::String ? string_ : empty;
    }

    const Array& as_array() const {
        static const Array empty;
        return kind_ == WireKind::Array ? array_ : empty;
    }

    bool operator==(const Value& other) const {
        if (kind_ != other.kind_) return false;
        switch (kind_) {
            case WireKind::None:
                return true;
            case WireKind::Bool:
                return boolean_ == other.boolean_;
            case WireKind::I64:
                return integer_ == other.integer_;
            case WireKind::String:
                return string_ == other.string_;
            case WireKind::Array:
                return array_ == other.array_;
            default:
                return number_ == other.number_;
        }
    }

    bool operator!=(const Value& other) const { return !(*this == other); }

private:
    static Value number(WireKind kind, double v) {
        Value value;
        value.kind_ = kind;
        value.number_ = v;
        return value;
    }

    WireKind kind_ = WireKind::None;
    double number_ = 0.0;
    int64_t integer_ = 0;
    bool boolean_ = false;
    std::string string_;
    Array array_;
};

class SchemaDef;

/// Base state row. Generated structs inherit and override the accessors.
struct Entity {
    uint32_t id = 0;
    const SchemaDef* definition = nullptr;

    virtual ~Entity() = default;
    virtual int field_count() const = 0;
    virtual void set_field(int field, const Value& value) = 0;
    virtual Value get_field(int field) const = 0;
    virtual void clear_fields() = 0;

    /// Reads a field by name, or null when the schema has no such field.
    Value get(const std::string& name) const;
    bool has(const std::string& name) const;
};

/// A collection schema decoded from the host descriptor.
struct SchemaDef {
    std::string name;
    uint32_t version = 0;
    uint32_t hash = 0;
    std::vector<FieldDef> fields;

    int index_of(const std::string& field_name) const {
        for (size_t i = 0; i < fields.size(); i++) {
            if (fields[i].name == field_name) return static_cast<int>(i);
        }
        return -1;
    }
};

inline Value Entity::get(const std::string& name) const {
    int index = definition ? definition->index_of(name) : -1;
    return index < 0 ? Value::null() : get_field(index);
}

inline bool Entity::has(const std::string& name) const {
    return definition != nullptr && definition->index_of(name) >= 0;
}

/// Untyped row used when no generated class is registered.
struct DynamicEntity final : Entity {
    explicit DynamicEntity(size_t field_count = 0) : values(field_count) {}

    int field_count() const override { return static_cast<int>(values.size()); }

    void set_field(int field, const Value& value) override {
        if (field >= 0 && static_cast<size_t>(field) < values.size()) values[field] = value;
    }

    Value get_field(int field) const override {
        return field >= 0 && static_cast<size_t>(field) < values.size() ? values[field] : Value::null();
    }

    void clear_fields() override { std::fill(values.begin(), values.end(), Value::null()); }

    std::vector<Value> values;
};

/// Maps schema names to generated entity constructors.
class Registry {
public:
    using Factory = std::function<std::unique_ptr<Entity>()>;

    void add(std::string schema_name, Factory factory) {
        if (schema_name.empty() || !factory) return;
        factories_[std::move(schema_name)] = std::move(factory);
    }

    std::unique_ptr<Entity> create(const SchemaDef& definition) const {
        std::unique_ptr<Entity> entity;
        auto found = factories_.find(definition.name);
        if (found != factories_.end() && found->second) entity = found->second();
        if (!entity) entity = std::make_unique<DynamicEntity>(definition.fields.size());
        entity->definition = &definition;
        entity->id = 0;
        entity->clear_fields();
        return entity;
    }

private:
    std::unordered_map<std::string, Factory> factories_;
};

/// One root collection: entities keyed by their wire id.
struct Collection {
    std::string name;
    const SchemaDef* definition = nullptr;
    std::unordered_map<uint32_t, std::unique_ptr<Entity>> entities;
    std::vector<uint32_t> order;
    bool ready = false;

    Entity* find(uint32_t id) const {
        auto found = entities.find(id);
        return found == entities.end() ? nullptr : found->second.get();
    }

    void remove_order(uint32_t id) {
        order.erase(std::remove(order.begin(), order.end(), id), order.end());
    }
};

/// A root map name and the schema it carries.
struct CollectionDef {
    std::string name;
    std::string schema;
};

/// The decoded `cyg.schema.bin` payload.
struct RootDef {
    uint8_t format_version = 0;
    std::string name;
    uint32_t version = 0;
    uint32_t hash = 0;
    std::vector<FieldDef> scalars;
    std::vector<CollectionDef> collections;
    std::vector<SchemaDef> schemas;

    const SchemaDef* find(const std::string& schema_name) const {
        for (const auto& schema : schemas) {
            if (schema.name == schema_name) return &schema;
        }
        return nullptr;
    }
};

/// Little-endian, bounds-checked reader shared by both decoders.
class Reader {
public:
    Reader(const uint8_t* data, size_t size, size_t offset = 0)
        : data_(data), size_(size), offset_(offset) {}

    bool failed() const { return failed_; }
    size_t offset() const { return offset_; }
    size_t size() const { return size_; }
    const uint8_t* data() const { return data_; }

    void fail() { failed_ = true; }

    bool skip(size_t count) {
        if (!need(count)) return false;
        offset_ += count;
        return true;
    }

    bool need(size_t count) {
        if (failed_) return false;
        if (count > size_ || offset_ > size_ - count) {
            failed_ = true;
            return false;
        }
        return true;
    }

    uint8_t u8() {
        if (!need(1)) return 0;
        return data_[offset_++];
    }

    int8_t i8() { return static_cast<int8_t>(u8()); }

    uint16_t u16() {
        if (!need(2)) return 0;
        uint16_t value = static_cast<uint16_t>(data_[offset_]) |
                         static_cast<uint16_t>(static_cast<uint16_t>(data_[offset_ + 1]) << 8);
        offset_ += 2;
        return value;
    }

    int16_t i16() { return static_cast<int16_t>(u16()); }

    uint32_t u32() {
        if (!need(4)) return 0;
        uint32_t value = static_cast<uint32_t>(data_[offset_]) |
                         (static_cast<uint32_t>(data_[offset_ + 1]) << 8) |
                         (static_cast<uint32_t>(data_[offset_ + 2]) << 16) |
                         (static_cast<uint32_t>(data_[offset_ + 3]) << 24);
        offset_ += 4;
        return value;
    }

    int32_t i32() { return static_cast<int32_t>(u32()); }

    int64_t i64() {
        uint64_t low = u32();
        uint64_t high = u32();
        return static_cast<int64_t>((high << 32) | low);
    }

    float f32() {
        uint32_t bits = u32();
        float value = 0.0f;
        std::memcpy(&value, &bits, sizeof(value));
        return value;
    }

    double f64() {
        uint64_t bits = static_cast<uint64_t>(i64());
        double value = 0.0;
        std::memcpy(&value, &bits, sizeof(value));
        return value;
    }

    std::string string8() {
        size_t length = u8();
        if (failed_ || !need(length)) return std::string();
        std::string value(reinterpret_cast<const char*>(data_ + offset_), length);
        offset_ += length;
        return value;
    }

    std::string string16() {
        size_t length = u16();
        if (failed_ || !need(length)) return std::string();
        std::string value(reinterpret_cast<const char*>(data_ + offset_), length);
        offset_ += length;
        return value;
    }

private:
    const uint8_t* data_ = nullptr;
    size_t size_ = 0;
    size_t offset_ = 0;
    bool failed_ = false;
};

/// Decodes the `OXSD` descriptor sent by the host on join.
class Descriptor {
public:
    static constexpr int kVersion = 3;

    static std::optional<RootDef> parse(const uint8_t* data, size_t size) {
        if (!data || size < 6) return std::nullopt;
        Reader reader(data, size);
        if (reader.u8() != 'O' || reader.u8() != 'X' || reader.u8() != 'S' || reader.u8() != 'D') {
            return std::nullopt;
        }
        uint8_t version = reader.u8();
        if (reader.failed() || version < 1 || version > kVersion) return std::nullopt;
        reader.u8(); // flags

        uint16_t schema_count = reader.u16();
        if (reader.failed()) return std::nullopt;
        RootDef root;
        root.format_version = version;
        root.schemas.resize(schema_count);
        for (auto& schema : root.schemas) {
            schema.name = reader.string16();
            schema.version = reader.u32();
            schema.hash = reader.u32();
            uint16_t field_count = reader.u16();
            if (reader.failed()) return std::nullopt;
            schema.fields.resize(field_count);
            for (auto& field : schema.fields) {
                if (!read_field(reader, version, field)) return std::nullopt;
            }
        }

        std::string root_name = reader.string16();
        if (reader.failed()) return std::nullopt;

        if (version == 1) {
            uint16_t index = reader.u16();
            if (reader.failed()) return std::nullopt;
            root.name = root_name;
            if (!root_name.empty() && index < root.schemas.size()) {
                root.collections.push_back({root_name, root.schemas[index].name});
            }
        } else {
            root.version = reader.u32();
            root.hash = reader.u32();
            uint16_t scalar_count = reader.u16();
            if (reader.failed()) return std::nullopt;
            root.scalars.resize(scalar_count);
            for (auto& field : root.scalars) {
                if (!read_field(reader, version, field)) return std::nullopt;
            }
            uint16_t collection_count = reader.u16();
            if (reader.failed()) return std::nullopt;
            root.collections.resize(collection_count);
            for (auto& collection : root.collections) {
                collection.name = reader.string16();
                uint16_t index = reader.u16();
                if (reader.failed()) return std::nullopt;
                if (index >= root.schemas.size()) return std::nullopt;
                collection.schema = root.schemas[index].name;
            }
            root.name = root_name;
        }

        if (reader.offset() != size) return std::nullopt;
        return root;
    }

    static std::optional<RootDef> parse(const std::vector<uint8_t>& bytes) {
        return parse(bytes.data(), bytes.size());
    }

private:
    static bool read_field(Reader& reader, uint8_t version, FieldDef& field) {
        field.name = reader.string16();
        uint8_t code = reader.u8();
        if (reader.failed() || !wire_kind_from_code(code, field.kind)) return false;
        if (version >= 3) {
            field.flags = reader.u8();
            uint8_t element_code = reader.u8();
            if (reader.failed()) return false;
            if (element_code != 0 && !wire_kind_from_code(element_code, field.element)) return false;
        }
        field.size = wire_kind_size(field.kind);
        return true;
    }
};

/// Applies one `OXS2` section (full snapshot or field patch) to the state.
class State {
public:
    State(const RootDef& root, Registry* registry = nullptr)
        : root_(root), registry_(registry ? registry : &default_registry_) {
        collections_.reserve(root_.collections.size());
        for (const auto& def : root_.collections) {
            const SchemaDef* schema = root_.find(def.schema);
            if (!schema) throw std::invalid_argument("unknown schema '" + def.schema + "'");
            Collection collection;
            collection.name = def.name;
            collection.definition = schema;
            collections_.push_back(std::move(collection));
        }

        root_definition_.name = root_.name;
        root_definition_.version = root_.version;
        root_definition_.hash = root_.hash;
        root_definition_.fields = root_.scalars;
        root_entity_ = registry_->create(root_definition_);
    }

    /// Parses an `OXSD` descriptor and builds its state.
    static std::optional<State> from_descriptor(const uint8_t* data, size_t size,
                                                Registry* registry = nullptr) {
        auto root = Descriptor::parse(data, size);
        if (!root) return std::nullopt;
        try {
            return State(*root, registry);
        } catch (const std::invalid_argument&) {
            return std::nullopt;
        }
    }

    static std::optional<State> from_descriptor(const std::vector<uint8_t>& bytes,
                                                Registry* registry = nullptr) {
        return from_descriptor(bytes.data(), bytes.size(), registry);
    }

    const RootDef& root() const { return root_; }
    Entity& root_entity() const { return *root_entity_; }
    bool ready() const { return ready_; }

    std::vector<Collection>& collections() { return collections_; }
    const std::vector<Collection>& collections() const { return collections_; }

    Collection* collection(const std::string& name) {
        for (auto& entry : collections_) {
            if (entry.name == name) return &entry;
        }
        return nullptr;
    }

    const Collection* collection(const std::string& name) const {
        for (const auto& entry : collections_) {
            if (entry.name == name) return &entry;
        }
        return nullptr;
    }

    using EntityHandler = std::function<void(const std::string&, uint32_t, Entity&)>;
    using FieldHandler =
        std::function<void(const std::string&, uint32_t, int, const Value&, const Value&)>;
    using RootFieldHandler = std::function<void(int, const Value&, const Value&)>;

    EntityHandler on_entity_added;
    EntityHandler on_entity_removed;
    EntityHandler on_entity_updated;
    FieldHandler on_field_changed;
    RootFieldHandler on_root_field_changed;

    /// Apply one section. A malformed or unknown frame is ignored.
    bool apply(const uint8_t* data, size_t size) {
        Reader reader(data, size);
        if (!is_magic(reader)) return false;
        uint8_t kind = reader.u8();
        if (reader.failed()) return false;
        if (kind == 1 || kind == 2) {
            if (collections_.empty()) return false;
            return apply_body(kind, collections_[0], reader);
        }
        if (kind == 3 || kind == 4) return apply_world(kind, reader);
        return false;
    }

    bool apply(const std::vector<uint8_t>& section) {
        return apply(section.data(), section.size());
    }

private:
    static bool is_magic(Reader& reader) {
        return reader.u8() == 'O' && reader.u8() == 'X' && reader.u8() == 'S' &&
               reader.u8() == '2' && !reader.failed();
    }

    bool apply_body(uint8_t kind, Collection& collection, Reader& reader) {
        return kind == 1 ? apply_full(collection, reader) : apply_patch(collection, reader);
    }

    bool apply_section(Collection& collection, Reader& reader, uint8_t expected) {
        if (!is_magic(reader)) return false;
        uint8_t kind = reader.u8();
        if (reader.failed() || kind != expected) return false;
        return apply_body(kind, collection, reader);
    }

    bool apply_full(Collection& collection, Reader& reader) {
        uint32_t hash = reader.u32();
        if (reader.failed() || hash != collection.definition->hash) return false;
        uint32_t count = reader.u32();
        if (reader.failed()) return false;

        const auto& fields = collection.definition->fields;
        std::vector<std::pair<uint32_t, std::vector<Value>>> incoming;
        incoming.reserve(std::min<size_t>(count, 65536));
        std::unordered_set<uint32_t> ids;
        ids.reserve(std::min<size_t>(count, 65536));
        for (uint32_t i = 0; i < count; i++) {
            uint32_t id = reader.u32();
            if (reader.failed()) return false;
            if (!ids.insert(id).second) return false;
            std::vector<Value> values(fields.size());
            for (size_t field = 0; field < fields.size(); field++) {
                values[field] = read_value(reader, fields[field]);
                if (reader.failed()) return false;
            }
            incoming.emplace_back(id, std::move(values));
        }

        for (size_t i = collection.order.size(); i-- > 0;) {
            uint32_t id = collection.order[i];
            if (ids.count(id)) continue;
            auto found = collection.entities.find(id);
            if (found == collection.entities.end()) continue;
            Entity& entity = *found->second;
            if (on_entity_removed) on_entity_removed(collection.name, id, entity);
            collection.entities.erase(found);
            collection.order.erase(collection.order.begin() + static_cast<std::ptrdiff_t>(i));
        }

        for (auto& entry : incoming) {
            uint32_t id = entry.first;
            auto found = collection.entities.find(id);
            if (found != collection.entities.end()) {
                apply_values(collection, id, *found->second, entry.second, true);
                continue;
            }
            auto entity = registry_->create(*collection.definition);
            entity->id = id;
            apply_values(collection, id, *entity, entry.second, false);
            Entity& stored = *entity;
            collection.entities.emplace(id, std::move(entity));
            collection.order.push_back(id);
            if (on_entity_added) on_entity_added(collection.name, id, stored);
        }
        collection.ready = true;
        return true;
    }

    bool apply_patch(Collection& collection, Reader& reader) {
        uint32_t hash = reader.u32();
        if (reader.failed() || hash != collection.definition->hash) return false;
        // A patch before the full row would be lost; the host keeps sending
        // full snapshots until the client acknowledges the baseline.
        if (!collection.ready) return false;
        uint32_t operations = reader.u32();
        if (reader.failed()) return false;

        const auto& fields = collection.definition->fields;
        for (uint32_t i = 0; i < operations; i++) {
            uint8_t op = reader.u8();
            if (reader.failed()) return false;
            if (op == 1) {
                uint32_t id = reader.u32();
                if (reader.failed()) return false;
                std::vector<Value> values(fields.size());
                for (size_t field = 0; field < fields.size(); field++) {
                    values[field] = read_value(reader, fields[field]);
                    if (reader.failed()) return false;
                }
                auto found = collection.entities.find(id);
                if (found != collection.entities.end()) {
                    apply_values(collection, id, *found->second, values, true);
                } else {
                    auto entity = registry_->create(*collection.definition);
                    entity->id = id;
                    apply_values(collection, id, *entity, values, false);
                    Entity& stored = *entity;
                    collection.entities.emplace(id, std::move(entity));
                    collection.order.push_back(id);
                    if (on_entity_added) on_entity_added(collection.name, id, stored);
                }
            } else if (op == 2) {
                uint32_t id = reader.u32();
                if (reader.failed()) return false;
                auto found = collection.entities.find(id);
                if (found != collection.entities.end()) {
                    Entity& entity = *found->second;
                    if (on_entity_removed) on_entity_removed(collection.name, id, entity);
                    collection.entities.erase(found);
                    collection.remove_order(id);
                }
            } else if (op == 3) {
                uint32_t id = reader.u32();
                uint32_t mask = reader.u32();
                if (reader.failed()) return false;
                auto found = collection.entities.find(id);
                Entity* entity = found == collection.entities.end() ? nullptr : found->second.get();
                bool changed = false;
                for (size_t field = 0; field < fields.size(); field++) {
                    if (field >= 32 || (mask & (1u << field)) == 0) continue;
                    Value value = read_value(reader, fields[field]);
                    if (reader.failed()) return false;
                    if (!entity) continue;
                    Value previous = entity->get_field(static_cast<int>(field));
                    entity->set_field(static_cast<int>(field), value);
                    if (previous == value) continue;
                    changed = true;
                    if (on_field_changed) {
                        on_field_changed(collection.name, id, static_cast<int>(field), value,
                                         previous);
                    }
                }
                if (changed && on_entity_updated) on_entity_updated(collection.name, id, *entity);
            } else {
                return false;
            }
        }
        return true;
    }

    bool apply_world(uint8_t kind, Reader& reader) {
        // A world patch before the baseline would be lost; the host repeats
        // full frames until the client acknowledges.
        if (kind == 4 && !ready_) return false;

        uint32_t root_hash = reader.u32();
        uint16_t root_count = reader.u16();
        if (reader.failed()) return false;
        if (root_count != root_.scalars.size()) return false;
        if (root_definition_.hash != root_hash) return false;

        uint32_t mask = 0;
        if (kind == 4) {
            mask = reader.u32();
            if (reader.failed()) return false;
        }
        for (size_t field = 0; field < root_.scalars.size(); field++) {
            if (kind == 4 && (field >= 32 || (mask & (1u << field)) == 0)) continue;
            Value value = read_value(reader, root_.scalars[field]);
            if (reader.failed()) return false;
            Value previous = root_entity_->get_field(static_cast<int>(field));
            root_entity_->set_field(static_cast<int>(field), value);
            if (previous != value && on_root_field_changed) {
                on_root_field_changed(static_cast<int>(field), value, previous);
            }
        }

        uint16_t count = reader.u16();
        if (reader.failed()) return false;
        for (uint16_t i = 0; i < count; i++) {
            uint16_t collection_id = reader.u16();
            uint32_t hash = reader.u32();
            uint32_t length = reader.u32();
            if (reader.failed()) return false;
            if (collection_id >= collections_.size()) return false;
            Collection& collection = collections_[collection_id];
            if (collection.definition->hash != hash) return false;
            if (length > reader.size() - reader.offset()) return false;
            Reader section(reader.data() + reader.offset(), length);
            uint8_t expected = kind == 3 ? 1 : 2;
            if (!apply_section(collection, section, expected)) return false;
            if (section.offset() != length) return false;
            reader.skip(length);
        }
        if (reader.offset() != reader.size()) return false;
        if (kind == 3) ready_ = true;
        return true;
    }

    void apply_values(Collection& collection, uint32_t id, Entity& entity,
                      const std::vector<Value>& values, bool fire) {
        bool changed = false;
        for (size_t field = 0; field < values.size(); field++) {
            Value previous = entity.get_field(static_cast<int>(field));
            const Value& value = values[field];
            entity.set_field(static_cast<int>(field), value);
            if (previous == value) continue;
            changed = true;
            if (fire && on_field_changed) {
                on_field_changed(collection.name, id, static_cast<int>(field), value, previous);
            }
        }
        if (changed && fire && on_entity_updated) on_entity_updated(collection.name, id, entity);
    }

    static Value read_value(Reader& reader, const FieldDef& field) {
        if (field.optional()) {
            uint8_t present = reader.u8();
            if (reader.failed()) return Value::null();
            if (present == 0) return Value::null();
        }
        if (field.kind != WireKind::Array) return read_scalar(reader, field.kind);

        WireKind element = field.element == WireKind::None ? WireKind::F64 : field.element;
        size_t count = reader.u8();
        if (reader.failed()) return Value::null();
        Value::Array values;
        values.reserve(count);
        for (size_t i = 0; i < count; i++) {
            values.push_back(read_scalar(reader, element));
            if (reader.failed()) return Value::null();
        }
        return Value::make_array(std::move(values));
    }

    static Value read_scalar(Reader& reader, WireKind kind) {
        switch (kind) {
            case WireKind::U8:
                return Value::make_u8(reader.u8());
            case WireKind::I8:
                return Value::make_i8(reader.i8());
            case WireKind::U16:
                return Value::make_u16(reader.u16());
            case WireKind::I16:
                return Value::make_i16(reader.i16());
            case WireKind::U32:
                return Value::make_u32(reader.u32());
            case WireKind::I32:
                return Value::make_i32(reader.i32());
            case WireKind::F32:
                return Value::make_f32(reader.f32());
            case WireKind::F64:
                return Value::make_f64(reader.f64());
            case WireKind::Bool:
                return Value::make_bool(reader.u8() != 0);
            case WireKind::I64:
                return Value::make_i64(reader.i64());
            case WireKind::String:
                return Value::make_string(reader.string8());
            default:
                reader.fail();
                return Value::null();
        }
    }

    RootDef root_;
    Registry default_registry_;
    Registry* registry_ = nullptr;
    SchemaDef root_definition_;
    std::unique_ptr<Entity> root_entity_;
    std::vector<Collection> collections_;
    bool ready_ = false;
};

}  // namespace cyg::schema
