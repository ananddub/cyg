// CYG schema runtime (C++17, header-only).
//
// The typed row/value layer for generated bindings (`cyg_schema_generated.hpp`):
// they subclass `Entity` and register factories with `Registry`.
//
//   Registry registry;
//   cyg::schema::generated::register_generated(registry);
//   auto player = registry.create(Player::kFields, ...);
//
// State decoding is NOT here. Client state is decoded only by `cyg-core-rs`
// (the shared native/wasm core); this header used to carry a second OXS2
// decoder, which has been removed so there is exactly one wire implementation.
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

}  // namespace cyg::schema
