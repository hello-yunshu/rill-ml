include $(TOPDIR)/rules.mk

PKG_NAME:=rill-runtime
PKG_VERSION:=1.5.5
PKG_RELEASE:=1
PKG_LICENSE:=MIT
PKG_MAINTAINER:=Rill contributors
PKG_SOURCE:=v$(PKG_VERSION).tar.gz
PKG_SOURCE_URL:=https://github.com/hello-yunshu/rill-ml/archive/refs/tags/
PKG_HASH:=dd35ff605a68f4285c0aa7c09c14f4f29bf17dfbde03c05c4faa21a867992361
PKG_BUILD_DIR:=$(BUILD_DIR)/rill-ml-$(PKG_VERSION)

PKG_BUILD_DEPENDS:=rust/host

include $(INCLUDE_DIR)/package.mk
include $(TOPDIR)/feeds/packages/lang/rust/rust-package.mk

define Package/rill-runtime
  SECTION:=utils
  CATEGORY:=Utilities
  TITLE:=Rill generic local runtime
  URL:=https://github.com/hello-yunshu/rill-ml
  DEPENDS:=+libc
  PROVIDES:=rill-runtime
endef

define Package/rill-runtime/description
Generic Rill Runtime packaged for OpenWrt. This recipe is distribution
packaging only: it does not add OpenWrt product semantics or host mutation.
The default upstream runtime features remain enabled and only rill-runtime,
not rill-pack, is installed on the target.
endef

define Build/Compile
	$(call RustPackage/Build/Compile)
endef

define Package/rill-runtime/install
	$(INSTALL_DIR) $(1)/usr/bin
	$(INSTALL_BIN) $(PKG_INSTALL_DIR)/usr/bin/rill-runtime $(1)/usr/bin/rill-runtime
endef

$(eval $(call BuildPackage,rill-runtime))
