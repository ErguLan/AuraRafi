// CONTRACT SKETCH ONLY: not an executable engine script or a shipping SDK.
// C++/WASM loading is deliberately not implemented in the local player.
// A future sandbox adapter must delegate these operations to the same Host API.
#include <array>
#include <string_view>

struct EntityRef { unsigned long long opaque = 0; };
struct CameraHost {
    virtual ~CameraHost() = default;
    virtual bool is_client() const = 0;
    virtual EntityRef entity(std::string_view stable_reference) = 0;
    virtual bool valid(EntityRef) const = 0;
    virtual void add_camera(EntityRef) = 0;
    virtual void activate_camera(EntityRef) = 0;
    virtual EntityRef active_camera() = 0;
    virtual void follow(EntityRef camera, EntityRef target,
                        std::array<float,3> offset, std::array<float,3> aim,
                        float sharpness, float dt) = 0;
};
void on_start(CameraHost& host) {
    if (!host.is_client()) return;
    auto camera = host.entity("/World/Camera");
    if (!host.valid(camera)) return;
    host.add_camera(camera);
    host.activate_camera(camera);
}
void on_late_update(CameraHost& host, float dt) {
    if (!host.is_client()) return;
    auto camera = host.active_camera();
    auto target = host.entity("/World/Player");
    if (!host.valid(camera) || !host.valid(target)) return;
    host.follow(camera, target, {0,2,8}, {0,1,0}, 8, dt);
}
