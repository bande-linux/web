/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef mozilla_widget_GSettings_h
#define mozilla_widget_GSettings_h

#include "nsString.h"
#include "nsTArray.h"

namespace mozilla::widget::GSettings {

class Collection final {
 public:
  explicit Collection(const nsACString&) {}
  explicit operator bool() const { return false; }
  bool GetString(const nsACString&, nsACString&) { return false; }
  bool GetStringList(const nsACString&, nsTArray<nsCString>&) { return false; }
};

inline bool GetString(const nsACString&, const nsACString&, nsACString&) {
  return false;
}

}  // namespace mozilla::widget::GSettings

#endif
