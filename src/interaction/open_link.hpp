#pragma once

#include <filesystem>
#include <string_view>

namespace booth {

/// True for `http`, `https` and `mailto` URLs with no embedded controls or
/// whitespace. Everything else is rejected before a browser is spawned.
bool is_openable_url(std::string_view url);

/// Opens an accepted URL with the platform opener (`xdg-open`/`open`) without a
/// shell. Never blocks on the browser: a short-lived intermediate process is
/// reaped instead. Returns false for rejected URLs or when the launcher cannot
/// be created.
bool open_url(std::string_view url);

/// Opens one already-validated local path with the platform opener. The caller
/// owns confinement (for example, inside the booth state directory).
bool open_path(const std::filesystem::path& path);

}  // namespace booth
