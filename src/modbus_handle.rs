use crate::device::Device;
use crate::device::ModbusDevice;
use crate::request::RequestFunction;

pub enum ModbusException {
    IllegalFunction = 1,
    IllegalDataAdress = 2,
    IllegalDataValue = 3,
    ServerDeviceFailure = 4,
    Acknowledge = 5,
    ServerDeviceBusy = 6,
    MemoryParityError = 7,
    GatewayPathUnavailable = 8,
    GatewayTargetDeviceFailedToRespond = 9,
}

struct Translate {}
