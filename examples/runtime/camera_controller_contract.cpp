// CONTRACT SKETCH ONLY: not an executable engine script or a shipping SDK.
// C++/WASM loading is deliberately not implemented in the local player.
// A future sandbox adapter must delegate these operations to the same Host API.
#include <array>
#include <string_view>

struct EntityRef { unsigned long long opaque = 0; };
struct CameraHost {
    virtual ~CameraHost() = default;
    virtual bool is_client() const = 0;
    virtual EntityRef owner() = 0; // Attachment owner, never hierarchy parent.
    virtual EntityRef parent(EntityRef) = 0;
    virtual EntityRef entity_parameter(std::string_view name) = 0;
    virtual float parameter(std::string_view name, float fallback) = 0;
    virtual EntityRef entity(std::string_view stable_reference) = 0;
    virtual bool valid(EntityRef) const = 0;
    virtual bool has_camera(EntityRef) const = 0;
    virtual void add_camera(EntityRef) = 0;
    virtual void activate_camera(EntityRef) = 0;
    virtual EntityRef active_camera() = 0;
    virtual void follow(EntityRef camera, EntityRef target,
                        std::array<float,3> offset, std::array<float,3> aim,
                        float sharpness, float dt) = 0;
};
void on_start(CameraHost& host) {
    if (!host.is_client()) return;
    auto camera = host.entity_parameter("camera");
    if (!host.valid(camera) || !host.has_camera(camera)) return;
    host.activate_camera(camera);
}
void on_late_update(CameraHost& host, float dt) {
    if (!host.is_client()) return;
    auto camera = host.active_camera();
    auto target = host.entity_parameter("target");
    if (!host.valid(camera) || !host.valid(target)) return;
    auto configured = host.entity_parameter("camera");
    // This sketch deliberately has no runtime adapter. A real implementation
    // must compare scoped identities before following a selected alternate view.
    if (camera.opaque != configured.opaque) return;
    host.follow(camera, target,
                {host.parameter("offset_x", 0), host.parameter("offset_y", 2), host.parameter("offset_z", 8)},
                {0, host.parameter("aim_height", 1), 0}, host.parameter("sharpness", 8), dt);
}
