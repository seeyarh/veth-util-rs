use futures::stream::TryStreamExt;
use rtnetlink::Handle;
use tokio::task::JoinHandle;
use mac_address::mac_address_by_name;


#[derive(Debug)]
pub struct VethPair {
    link_handle: Handle,
    join_handle: JoinHandle<()>,
    rt: tokio::runtime::Runtime,
    dev1: VethLink,
    dev2: VethLink,
}

impl VethPair {
    pub fn dev1(&self) -> &VethLink {
        &self.dev1
    }

    pub fn dev2(&self) -> &VethLink {
        &self.dev2
    }
}

#[derive(Debug)]
pub struct VethLink {
    ifname: String,
    index: u32,
    mac_addr: [u8; 6],
}

impl VethLink {
    pub fn ifname(&self) -> &str {
        &self.ifname
    }

    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn mac_addr(&self) -> &[u8; 6] {
        &self.mac_addr
    }
}

#[derive(Debug)]
pub struct VethConfig {
    dev1_ifname: String,
    dev2_ifname: String,
}

impl VethConfig {
    pub fn new(dev1_ifname: String, dev2_ifname: String) -> Self {
        Self {dev1_ifname, dev2_ifname}
    }
}

impl Default for VethConfig {
    fn default() -> Self {
        Self {
            dev1_ifname: "veth0".into(),
            dev2_ifname: "veth1".into(),
        }
    }
}

impl Drop for VethPair {
    fn drop(&mut self) {
        self.rt.block_on(async {
            delete_link(&self.link_handle, self.dev1.index).await
        }).expect("failed to delete link");
    }
}

async fn delete_link(handle: &Handle, index: u32) -> anyhow::Result<()> {
    Ok(handle.link().del(index).execute().await?)
}

async fn get_link_index(handle: &Handle, name: &str) -> anyhow::Result<u32> {
    Ok(handle
        .link()
        .get()
        .set_name_filter(name.into())
        .execute()
        .try_next()
        .await?
        .expect(format!("No link with name {} found", name).as_str())
        .header
        .index)
}

async fn set_link_up(handle: &Handle, index: u32) -> anyhow::Result<()> {
    Ok(handle.link().set(index).up().execute().await?)
}

async fn setup_veth_link(veth_config: &VethConfig) -> anyhow::Result<(Handle, JoinHandle<()>, VethLink, VethLink)> {
        let (connection, link_handle, _) = rtnetlink::new_connection().expect("failed to create  rtnetlink connection");
        let join_handle = tokio::spawn(connection);

        link_handle
            .link()
            .add()
            .veth(veth_config.dev1_ifname.clone(), veth_config.dev2_ifname.clone())
            .execute()
            .await?;

        let dev1_index = get_link_index(&link_handle, &veth_config.dev1_ifname).await.expect(
            format!(
                "Failed to retrieve index, this is not expected. Remove link manually: 'sudo ip link del {}'",
                veth_config.dev1_ifname
            )
            .as_str(),
        );
        let dev2_index = get_link_index(&link_handle, &veth_config.dev2_ifname).await?;

        set_link_up(&link_handle, dev1_index).await?;
        set_link_up(&link_handle, dev2_index).await?;


        let mac1 = match mac_address_by_name(&veth_config.dev1_ifname) {
            Ok(Some(ma)) => {
                ma.bytes()
            }
            Ok(None) => {
                anyhow::bail!("no mac addr for interface");
            }
            Err(e) => {
                eprintln!("{:?}", e);
                anyhow::bail!("error retrieving mac addr");
            },
        };

        let mac2 = match mac_address_by_name(&veth_config.dev2_ifname) {
            Ok(Some(ma)) => {
                ma.bytes()
            }
            Ok(None) => {
                anyhow::bail!("no mac addr for interface");
            }
            Err(e) => {
                eprintln!("{:?}", e);
                anyhow::bail!("error retrieving mac addr");
            },
        };

        let dev1 = VethLink {
            ifname: veth_config.dev1_ifname.clone(),
            index: dev1_index,
            mac_addr: mac1,
        };

        let dev2 = VethLink {
            ifname: veth_config.dev2_ifname.clone(),
            index: dev2_index,
            mac_addr: mac2,
        };

        Ok((link_handle, join_handle, dev1, dev2))
}

pub fn add_veth_link(veth_config: &VethConfig) -> anyhow::Result<VethPair> {
    let rt = tokio::runtime::Runtime::new().expect("failed to build tokio runtime");

    let (link_handle, join_handle, dev1, dev2) = rt.block_on(async {
        setup_veth_link(veth_config).await
    })?;

    Ok(VethPair { link_handle, join_handle, rt, dev1, dev2})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let veth_config = VethConfig::default();
        let pair = add_veth_link(&veth_config).expect("failed to create veth pair");
        assert_eq!(pair.dev1().ifname(), veth_config.dev1_ifname);
        assert_eq!(pair.dev2().ifname(), veth_config.dev2_ifname);

        pair.dev1().index();
        pair.dev2().index();
        pair.dev1().mac_addr();
        pair.dev2().mac_addr();
    }
}
