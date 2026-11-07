/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef MOZ_CONTAINER_H_
#define MOZ_CONTAINER_H_

#include "mozilla/RefPtr.h"
#include "mozilla/widget/WaylandSurface.h"

struct MozContainer {
  RefPtr<mozilla::widget::WaylandSurface> mSurface;
};

#define MOZ_WL_SURFACE(obj) ((obj)->mSurface.get())

#endif
