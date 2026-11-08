/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef widget_bande_ScreenHelperBande_h
#define widget_bande_ScreenHelperBande_h

class ScreenHelperBande final {
 public:
  static void EnsureStarted();
  static void Refresh();
  static void SetScale(double aScale);
};

#endif
