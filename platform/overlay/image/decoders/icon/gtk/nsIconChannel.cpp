/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "nsIconChannel.h"

#include "mozilla/gfx/2D.h"
#include "mozilla/ipc/ByteBuf.h"

NS_IMPL_ISUPPORTS(nsIconChannel, nsIRequest, nsIChannel)

nsresult nsIconChannel::Init(nsIURI*, nsILoadInfo*) {
  return NS_ERROR_NOT_AVAILABLE;
}

nsresult nsIconChannel::GetIcon(nsIURI*, mozilla::ipc::ByteBuf*) {
  return NS_ERROR_NOT_AVAILABLE;
}

already_AddRefed<mozilla::gfx::DataSourceSurface>
nsIconChannel::GetSymbolicIcon(const nsCString&, int, int, nscolor) {
  return nullptr;
}

void nsIconChannel::Shutdown() {}
